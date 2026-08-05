//! Acceptance check against the player's real `Client.txt`.
//!
//! This is not part of the normal suite: it depends on a file that exists only
//! on this machine, so it is `#[ignore]`d and must be run explicitly with
//! `cargo test --test real_log -- --ignored --nocapture`. It mirrors what
//! `poll_once` in `src-tauri/src/poe2/tracker.rs` does, minus the Tauri
//! dependency, and reports the resulting counts so they can be compared
//! against the Python original's database (about 14,474 events, 140 zones, 19
//! characters, 1 generation).
//!
//! The log is opened read-only and never written to.

use std::time::Instant;

use poe2_core::log::events::EventKind;
use poe2_core::log::parser::parse_line;
use poe2_core::log::state::{build_state, level_gap};
use poe2_core::log::tail::LogTail;
use poe2_core::log::zones::ZoneResolver;
use poe2_core::store::{IngestBatch, Poe2Store};

const LOG_PATH: &str =
    r"C:\Program Files (x86)\Steam\steamapps\common\Path of Exile 2\logs\Client.txt";

#[test]
#[ignore]
fn ingesting_the_real_client_log_matches_the_python_original() {
    let path = std::path::Path::new(LOG_PATH);
    if !path.exists() {
        println!(
            "skipping: {LOG_PATH} does not exist on this machine (real-log check only runs \
             where the game is installed)"
        );
        return;
    }

    let started = Instant::now();

    // Same shape as poll_once: a fresh tail from offset 0, no prior fingerprint,
    // reading the whole file in one go.
    let mut tail = LogTail::new(path, 0, None);
    let lines = tail.read_new();
    let read_elapsed = started.elapsed();

    let mut zones = ZoneResolver::new();
    let mut batch = IngestBatch {
        events: Vec::new(),
        generation: 0,
        zones: Vec::new(),
        characters: Vec::new(),
        meta: Vec::new(),
    };

    let mut unparsed = 0usize;
    for (line_offset, line) in &lines {
        let Some(event) = parse_line(line) else {
            unparsed += 1;
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

    let parse_elapsed = started.elapsed();

    let mut store = Poe2Store::in_memory().expect("in-memory store");
    let added = store.ingest_batch(&batch).expect("ingest_batch");
    let ingest_elapsed = started.elapsed();

    let stored_events = store.events().expect("events");
    let zone_rows = store.zones().expect("zones");
    let character_rows = store.characters().expect("characters");
    let state = build_state(&stored_events);
    let total_elapsed = started.elapsed();

    println!("--- real log acceptance check ---");
    println!("log path: {LOG_PATH}");
    println!("lines read: {}", lines.len());
    println!("events added by ingest_batch: {added}");
    println!("events stored: {}", stored_events.len());
    println!("zones: {}", zone_rows.len());
    println!("characters: {}", character_rows.len());
    println!("unparsed lines: {unparsed}");
    println!("character: {:?}", state.character);
    println!("level: {:?}", state.level);
    println!("zone_code: {:?}", state.zone_code);
    println!(
        "zone_name: {:?}",
        zone_rows
            .iter()
            .find(|z| Some(&z.code) == state.zone_code.as_ref())
            .and_then(|z| z.name.clone())
    );
    println!("act: {:?}", state.act);
    println!("level_gap: {:?}", level_gap(&state));
    println!(
        "timing: read {:?}, parse (cumulative) {:?}, ingest (cumulative) {:?}, total {:?}",
        read_elapsed, parse_elapsed, ingest_elapsed, total_elapsed
    );
    println!("----------------------------------");

    // Generous bands: the log has grown since the Python original's numbers
    // were taken (14,474 events, 140 zones, 19 characters), so more is
    // expected and fine. What these guard against is drift an order of
    // magnitude off, which would mean the parser stopped matching the
    // original's behaviour.
    assert!(
        (10_000..25_000).contains(&stored_events.len()),
        "event count {} is outside the expected neighbourhood of 14,474",
        stored_events.len()
    );
    assert!(
        (100..250).contains(&zone_rows.len()),
        "zone count {} is outside the expected neighbourhood of 140",
        zone_rows.len()
    );
    assert!(
        (10..40).contains(&character_rows.len()),
        "character count {} is outside the expected neighbourhood of 19",
        character_rows.len()
    );
}
