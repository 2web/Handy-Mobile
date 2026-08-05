//! Tauri commands for the Path of Exile 2 section.

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;

use poe2_core::items::parse_item;
use poe2_core::log::state::{build_state, level_gap};
use poe2_core::store::{Poe2Store, StoredItem, ZoneRow};

use crate::settings;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct AddItemResult {
    pub id: i64,
    pub created: bool,
    pub item: Option<StoredItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct RebuildResult {
    pub reparsed: u32,
    pub failed: u32,
}

/// Opens the item database. A fresh connection per call: opening a SQLite file
/// is cheap, and a per-call connection keeps the commands free of shared state.
pub fn store_for(app: &AppHandle) -> Result<Poe2Store, String> {
    let dir = crate::portable::app_data_dir(app).map_err(|e| e.to_string())?;
    Poe2Store::open(&dir.join("poe2.db")).map_err(|e| e.to_string())
}

/// The fixed error text `poe2_add_item` returns when the pasted text does not
/// parse as an item. Exported so the frontend can match on it and show a
/// different message than a genuine storage failure — matching a known,
/// fixed string here is simpler and less brittle than parsing prose out of
/// an arbitrary storage error on the frontend.
pub const NOT_AN_ITEM_ERROR: &str =
    "That does not look like an item. Hover it in the game and press Ctrl+C.";

#[tauri::command]
#[specta::specta]
pub fn poe2_add_item(app: AppHandle, text: String) -> Result<AddItemResult, String> {
    let parsed = parse_item(&text).map_err(|_| NOT_AN_ITEM_ERROR.to_string())?;

    let mut store = store_for(&app)?;
    let (id, created) = store
        .add_item(&parsed, "paste", chrono::Utc::now())
        .map_err(|e| e.to_string())?;
    let item = store.item(id).map_err(|e| e.to_string())?;
    Ok(AddItemResult { id, created, item })
}

#[tauri::command]
#[specta::specta]
pub fn poe2_list_items(app: AppHandle) -> Result<Vec<StoredItem>, String> {
    store_for(&app)?.items(50).map_err(|e| e.to_string())
}

/// Reparses stored items from their raw text.
///
/// Raw text is the source of truth and structure is derived. When the parser
/// gets smarter — and it will — stored items reparse themselves with no
/// re-pasting.
#[tauri::command]
#[specta::specta]
pub fn poe2_rebuild_items(app: AppHandle) -> Result<RebuildResult, String> {
    let mut store = store_for(&app)?;
    let mut reparsed = 0u32;
    let mut failed = 0u32;
    for (id, raw_text) in store.raw_items().map_err(|e| e.to_string())? {
        // A record that no longer parses must not abort the rebuild of all the
        // others, but it also must not vanish silently: a raw text that stops
        // parsing means either the parser regressed or the row is corrupt, and
        // both are worth knowing about. Never log the raw text itself — it is
        // player data and the log is a file on disk.
        match parse_item(&raw_text) {
            Ok(parsed) => {
                store.reparse_item(id, &parsed).map_err(|e| e.to_string())?;
                reparsed += 1;
            }
            Err(_) => {
                log::warn!("poe2_rebuild_items: item {id} no longer parses, skipping");
                failed += 1;
            }
        }
    }
    Ok(RebuildResult { reparsed, failed })
}

/// What the Progress tab needs, flattened.
///
/// `TrackerState` itself does not cross this boundary: its timestamps are
/// `NaiveDateTime`, which specta cannot describe without its chrono feature. The
/// two timestamps the interface actually shows are rendered as ISO strings here.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ProgressSnapshot {
    pub character: Option<String>,
    pub ascendancy: Option<String>,
    pub level: Option<i64>,
    pub zone_code: Option<String>,
    pub zone_name: Option<String>,
    pub zone_level: Option<i64>,
    pub character_confirmed_ts: Option<String>,
    pub last_ts: Option<String>,
    pub focused: bool,
    pub rewards: Vec<String>,
    pub level_gap: Option<i64>,
    pub seconds_in_zone: Option<i64>,
    /// The current zone's act, which is more trustworthy than the last act seen:
    /// a global "last act" never resets and would show an act finished hours ago
    /// once the player reaches the endgame or a hideout.
    pub act: Option<String>,
    pub log_present: bool,
    pub debug_lines: bool,
    pub importing: bool,
    pub event_count: i64,
}

#[tauri::command]
#[specta::specta]
pub fn poe2_state(app: AppHandle) -> Result<ProgressSnapshot, String> {
    let store = store_for(&app)?;
    let events = store.events().map_err(|e| e.to_string())?;
    let state = build_state(&events);

    let zones: Vec<ZoneRow> = store.zones().map_err(|e| e.to_string())?;
    let zone = zones
        .iter()
        .find(|z| Some(z.code.as_str()) == state.zone_code.as_deref());

    let seconds_in_zone = state
        .zone_since
        .map(|since| (chrono::Local::now().naive_local() - since).num_seconds());

    let rewards = state
        .character
        .as_ref()
        .and_then(|name| state.rewards.get(name))
        .cloned()
        .unwrap_or_default();

    let path = crate::poe2::tracker::log_path(&app);
    Ok(ProgressSnapshot {
        character: state.character.clone(),
        ascendancy: state.ascendancy.clone(),
        level: state.level,
        zone_code: state.zone_code.clone(),
        zone_name: zone.and_then(|z| z.name.clone()),
        zone_level: state.zone_level,
        character_confirmed_ts: state.character_confirmed_ts.map(|t| t.to_string()),
        last_ts: state.last_ts.map(|t| t.to_string()),
        focused: state.focused,
        rewards,
        level_gap: level_gap(&state),
        seconds_in_zone,
        act: zone
            .and_then(|z| z.act.clone())
            .or_else(|| state.act.clone()),
        log_present: path.exists(),
        debug_lines: crate::poe2::tracker::has_debug_lines(&path),
        importing: crate::poe2::tracker::IMPORTING.load(std::sync::atomic::Ordering::SeqCst),
        event_count: events.len() as i64,
    })
}

/// Replays the stored events to rebuild `zones` and `characters`.
///
/// The event log itself is never touched. This exists so a fix to the zone
/// pairing rules can be applied to history that is already ingested.
#[tauri::command]
#[specta::specta]
pub fn poe2_rebuild_derived(app: AppHandle) -> Result<u32, String> {
    let mut store = store_for(&app)?;
    let events = store.events().map_err(|e| e.to_string())?;
    store.clear_derived().map_err(|e| e.to_string())?;

    let mut zones = poe2_core::log::zones::ZoneResolver::new();
    for event in &events {
        for update in zones.feed(event) {
            store
                .apply_zone_update(&update)
                .map_err(|e| e.to_string())?;
        }
        match event.kind {
            poe2_core::log::events::EventKind::LevelUp => {
                if let Some(name) = event.payload["character"].as_str() {
                    store
                        .upsert_character(
                            name,
                            event.payload["ascendancy"].as_str(),
                            Some(event.ts),
                        )
                        .map_err(|e| e.to_string())?;
                }
            }
            poe2_core::log::events::EventKind::QuestReward => {
                if let Some(name) = event.payload["character"].as_str() {
                    store
                        .upsert_character(name, None, Some(event.ts))
                        .map_err(|e| e.to_string())?;
                }
            }
            _ => {}
        }
    }
    Ok(events.len() as u32)
}

#[tauri::command]
#[specta::specta]
pub fn change_poe2_log_path_setting(app: AppHandle, path: Option<String>) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.poe2_log_path = path;
    settings::write_settings(&app, settings.clone());
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_poe2_enabled_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.poe2_enabled = enabled;
    settings::write_settings(&app, settings.clone());
    if enabled {
        // Mirrors change_poe2_clipboard_watch_setting: the watcher now gates
        // on both flags, so turning the section back on can leave both true
        // with no thread running (e.g. after enable -> disable section ->
        // restart -> re-enable). spawn's WATCHER_RUNNING compare-exchange
        // makes calling it from both commands safe.
        crate::poe2::watcher::spawn(app.clone());
        crate::poe2::tracker::spawn(app);
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_poe2_clipboard_watch_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.poe2_clipboard_watch = enabled;
    settings::write_settings(&app, settings.clone());
    if enabled {
        // Starts the polling thread immediately rather than making the user
        // restart the app to see the toggle they just turned on take effect.
        // `spawn` itself guards against a duplicate thread if one is already
        // running.
        crate::poe2::watcher::spawn(app);
    }
    Ok(())
}
