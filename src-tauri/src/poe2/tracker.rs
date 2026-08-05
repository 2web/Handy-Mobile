//! Ingesting Client.txt into the event log, and the thread that keeps it current.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

use poe2_core::log::events::EventKind;
use poe2_core::log::parser::parse_line;
use poe2_core::log::tail::LogTail;
use poe2_core::log::zones::ZoneResolver;
use poe2_core::store::IngestBatch;

use crate::poe2::commands::store_for;
use crate::settings;

pub const DEFAULT_LOG_PATH: &str =
    r"C:\Program Files (x86)\Steam\steamapps\common\Path of Exile 2\logs\Client.txt";

pub const POLL_INTERVAL: Duration = Duration::from_secs(1);
pub const STATE_CHANGED_EVENT: &str = "poe2://state-changed";

/// Set while the first, whole-file import is running, so the interface can say
/// so — a minute of blank fields is indistinguishable from a broken feature.
pub static IMPORTING: AtomicBool = AtomicBool::new(false);

static TRACKER_RUNNING: AtomicBool = AtomicBool::new(false);

pub fn log_path(app: &AppHandle) -> PathBuf {
    match settings::get_settings(app).poe2_log_path {
        Some(path) if !path.trim().is_empty() => PathBuf::from(path),
        _ => PathBuf::from(DEFAULT_LOG_PATH),
    }
}

/// Whether the log carries `[DEBUG]` lines at all.
///
/// The game omits them unless the player enables them, and without them zone
/// changes are invisible. Only the tail of the file is sampled: reading 27 MB to
/// answer a yes/no question would stall every poll.
pub fn has_debug_lines(path: &Path) -> bool {
    use std::io::{Read, Seek, SeekFrom};
    const SAMPLE: u64 = 2_000_000;

    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let start = metadata.len().saturating_sub(SAMPLE);
    if file.seek(SeekFrom::Start(start)).is_err() {
        return false;
    }
    let mut buf = Vec::new();
    if file.take(SAMPLE).read_to_end(&mut buf).is_err() {
        return false;
    }
    String::from_utf8_lossy(&buf).contains("[DEBUG Client")
}

/// How long a cached `has_debug_lines` result is trusted before being
/// recomputed.
const DEBUG_LINES_CACHE_TTL: Duration = Duration::from_secs(60);

static DEBUG_LINES_CACHE: Mutex<Option<(PathBuf, Instant, bool)>> = Mutex::new(None);

/// Cached `has_debug_lines`, recomputed at most once per minute per path.
///
/// `poe2_state` calls this roughly once a second while the player is active,
/// and the answer only changes when the log path changes or the player edits
/// the game's own settings — recomputing it that often means reading 2 MB off
/// disk for no reason on almost every call.
pub fn has_debug_lines_cached(path: &Path) -> bool {
    let mut cache = DEBUG_LINES_CACHE.lock().unwrap();
    if let Some((cached_path, checked_at, value)) = cache.as_ref() {
        if cached_path == path && checked_at.elapsed() < DEBUG_LINES_CACHE_TTL {
            return *value;
        }
    }
    let value = has_debug_lines(path);
    *cache = Some((path.to_path_buf(), Instant::now(), value));
    value
}

/// Reads whatever is new and stores it. Returns how many events were added.
///
/// `zones` must outlive a single poll: a `Generating level` line establishes
/// the pending zone, and the `Set Source` lines that name it and give its act
/// typically arrive in a *later* poll. A resolver rebuilt every call never
/// sees both halves of the pair. `spawn` owns one resolver for the life of
/// its loop and threads it through; a caller with no long-lived resolver
/// (e.g. a one-off import) can pass a fresh `ZoneResolver::new()`.
pub fn poll_once(app: &AppHandle, zones: &mut ZoneResolver) -> Result<u32, String> {
    let path = log_path(app);
    let mut store = store_for(app)?;

    let offset: u64 = store
        .get_meta("log_offset")
        .map_err(|e| e.to_string())?
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let mut generation: i64 = store
        .get_meta("log_generation")
        .map_err(|e| e.to_string())?
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    // An empty string means "no fingerprint", which is not the same as "the
    // fingerprint of the previous file" — see LogTail for why that distinction
    // matters after a restart.
    let fingerprint = store
        .get_meta("log_fingerprint")
        .map_err(|e| e.to_string())?
        .filter(|f| !f.is_empty());

    let mut tail = LogTail::new(&path, offset, fingerprint);
    let lines = tail.read_new();
    if tail.rotated {
        generation += 1;
    }
    if lines.is_empty() {
        return Ok(0);
    }

    // The whole poll is assembled first and written in one transaction. Writing
    // per event turned the cold import of a 27 MB log into ninety seconds of
    // waiting on the disk, and it let the read offset advance past events that
    // had not been stored.
    let mut batch = IngestBatch {
        events: Vec::new(),
        generation,
        zones: Vec::new(),
        characters: Vec::new(),
        meta: Vec::new(),
    };

    for (line_offset, line) in &lines {
        let Some(event) = parse_line(line) else {
            continue;
        };
        batch.zones.extend(zones.feed(&event));
        match event.kind {
            EventKind::LevelUp => {
                if let Some(name) = event.payload["character"].as_str() {
                    batch.characters.push((
                        name.to_string(),
                        event.payload["ascendancy"].as_str().map(str::to_string),
                        Some(event.ts),
                    ));
                }
            }
            EventKind::QuestReward => {
                if let Some(name) = event.payload["character"].as_str() {
                    batch
                        .characters
                        .push((name.to_string(), None, Some(event.ts)));
                }
            }
            _ => {}
        }
        batch.events.push((event, *line_offset as i64));
    }

    batch
        .meta
        .push(("log_offset".into(), tail.offset.to_string()));
    batch
        .meta
        .push(("log_generation".into(), generation.to_string()));
    // An empty string means "no fingerprint" and is stored deliberately: leaving
    // the previous file's fingerprint in place would make a later, longer file
    // look like another rotation after a restart.
    batch.meta.push((
        "log_fingerprint".into(),
        tail.fingerprint.clone().unwrap_or_default(),
    ));

    store.ingest_batch(&batch).map_err(|e| e.to_string())
}

/// Starts the polling thread if the section is enabled. Safe to call repeatedly.
///
/// Like the clipboard watcher, the thread persists for the process and simply
/// stops reading while the section is off: a thread that exits on a setting
/// change races with a re-enable and can leave nothing running at all.
pub fn spawn(app: AppHandle) {
    if !settings::get_settings(&app).poe2_enabled {
        return;
    }
    if TRACKER_RUNNING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }

    std::thread::spawn(move || {
        // Lives for the whole loop: a `Generating level` line and the `Set
        // Source` lines that name it commonly land in different polls, and a
        // resolver rebuilt every call never sees both halves of the pair.
        let mut zones = ZoneResolver::new();

        // The first poll has the whole file to get through.
        IMPORTING.store(true, Ordering::SeqCst);
        let first = poll_once(&app, &mut zones);
        IMPORTING.store(false, Ordering::SeqCst);
        match first {
            Ok(n) if n > 0 => {
                let _ = app.emit(STATE_CHANGED_EVENT, ());
            }
            Err(e) => log::warn!("poe2 tracker: initial import failed: {e}"),
            _ => {}
        }

        loop {
            std::thread::sleep(POLL_INTERVAL);
            if !settings::get_settings(&app).poe2_enabled {
                continue;
            }
            match poll_once(&app, &mut zones) {
                Ok(0) => {}
                Ok(_) => {
                    let _ = app.emit(STATE_CHANGED_EVENT, ());
                }
                Err(e) => log::warn!("poe2 tracker: poll failed: {e}"),
            }
        }
    });
}
