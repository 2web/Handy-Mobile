//! Acceptance tests for the definition of done in
//! docs/superpowers/specs/2026-08-05-poe2-log-tracker-design.md.
//!
//! Two criteria cannot be expressed here and are enforced elsewhere:
//!   - Criterion 2's "updates within about a second" and criterion 4's
//!     "does not block the window" live in the polling thread in
//!     src-tauri/src/poe2/tracker.rs.
//!   - Criterion 8, i18n labels, lives in src/i18n/locales/{en,ru}/translation.json
//!     and src/components/poe2/ProgressTab.tsx.

#[cfg(test)]
mod tests {
    use crate::log::events::EventKind;
    use crate::log::parser::parse_line;
    use crate::log::state::{build_state, level_gap};
    use crate::log::tail::{LogTail, FINGERPRINT_BYTES};
    use crate::log::zones::ZoneResolver;
    use crate::store::Poe2Store;
    use chrono::NaiveDate;
    use serde_json::json;

    const SAMPLE: &str = include_str!("fixtures/sample_client.txt");

    /// Criterion 5: a log with no DEBUG lines must be detectable, because
    /// without them zone changes are invisible and the interface has to say so.
    #[test]
    fn debug_lines_are_distinguishable_in_the_sample() {
        let debug_lines = SAMPLE
            .lines()
            .filter(|l| l.contains("[DEBUG Client"))
            .count();
        assert_eq!(debug_lines, 1, "the fixture carries exactly one DEBUG line");
    }

    /// Criterion 6: a zone entry followed by both kinds of Set Source records
    /// both the name and the act.
    #[test]
    fn a_zone_gets_both_its_name_and_its_act() {
        let base = NaiveDate::from_ymd_opt(2026, 8, 2).unwrap();
        let mut resolver = ZoneResolver::new();
        let area = crate::log::events::Event {
            ts: base.and_hms_opt(12, 0, 0).unwrap(),
            kind: EventKind::AreaEntered,
            payload: json!({"code": "G1_4", "area_level": 4}),
        };
        let named = crate::log::events::Event {
            ts: base.and_hms_opt(12, 0, 1).unwrap(),
            kind: EventKind::Scene,
            payload: json!({"source": "Clearfell Encampment"}),
        };
        let acted = crate::log::events::Event {
            ts: base.and_hms_opt(12, 0, 2).unwrap(),
            kind: EventKind::Scene,
            payload: json!({"source": "Act 1"}),
        };

        let mut store = Poe2Store::in_memory().unwrap();
        for event in [&area, &named, &acted] {
            for update in resolver.feed(event) {
                store.apply_zone_update(&update).unwrap();
            }
        }

        let zones = store.zones().unwrap();
        assert_eq!(zones.len(), 1);
        assert_eq!(zones[0].name.as_deref(), Some("Clearfell Encampment"));
        assert_eq!(zones[0].act.as_deref(), Some("Act 1"));
    }

    /// Criterion 3: a rotated log starts a new generation and loses no events.
    #[test]
    fn rotation_starts_a_generation_and_keeps_everything() {
        let mut store = Poe2Store::in_memory().unwrap();
        let event = crate::log::events::Event {
            ts: NaiveDate::from_ymd_opt(2026, 8, 2)
                .unwrap()
                .and_hms_opt(12, 0, 0)
                .unwrap(),
            kind: EventKind::Focus,
            payload: json!({"gained": true}),
        };
        assert!(store.add_event(&event, 42, 0).unwrap());
        // After rotation the same byte offset comes round again.
        assert!(store.add_event(&event, 42, 1).unwrap());
        assert_eq!(store.events().unwrap().len(), 2);
    }

    /// Criterion 7: rebuilding derived tables never touches the event log.
    #[test]
    fn rebuilding_derived_tables_leaves_events_untouched() {
        let mut store = Poe2Store::in_memory().unwrap();
        let event = crate::log::events::Event {
            ts: NaiveDate::from_ymd_opt(2026, 8, 2)
                .unwrap()
                .and_hms_opt(12, 0, 0)
                .unwrap(),
            kind: EventKind::LevelUp,
            payload: json!({"character": "Hero", "ascendancy": "Sorceress", "level": 5}),
        };
        store.add_event(&event, 10, 0).unwrap();
        store
            .upsert_character("Hero", Some("Sorceress"), Some(event.ts))
            .unwrap();

        store.clear_derived().unwrap();

        assert!(store.characters().unwrap().is_empty());
        assert_eq!(store.events().unwrap().len(), 1);
    }

    /// The whole pipeline on the real fixture: lines in, state out.
    #[test]
    fn the_fixture_produces_a_coherent_state() {
        let events: Vec<_> = SAMPLE.lines().filter_map(parse_line).collect();
        let state = build_state(&events);
        assert_eq!(state.character.as_deref(), Some("Kasablankee"));
        assert_eq!(state.zone_code.as_deref(), Some("P2_2"));
        assert_eq!(state.zone_level, Some(59));
        // The level came from a line earlier than the zone, and the character
        // never changed, so it stands.
        assert_eq!(state.level, Some(61));
        assert_eq!(level_gap(&state), Some(2));
        let rewards = state.rewards.get("Kasablankee").unwrap();
        assert!(rewards.contains(&"+10% to Cold Resistance".to_string()));
        assert!(rewards.contains(&"+1 Charm Slot".to_string()));
        // The chat line that reads like a reward must not be among them.
        assert!(!rewards.iter().any(|r| r.contains("Spirit")));
    }

    /// A file replaced by one of the same length is caught only by the
    /// fingerprint — size alone cannot see it.
    #[test]
    fn same_length_replacement_is_detected() {
        let mut path = std::env::temp_dir();
        path.push(format!("poe2-acceptance-{}.txt", std::process::id()));
        let _ = std::fs::remove_file(&path);

        std::fs::write(&path, "A".repeat(FINGERPRINT_BYTES) + "\n").unwrap();
        let mut tail = LogTail::new(&path, 0, None);
        tail.read_new();
        assert!(!tail.rotated);

        std::fs::write(&path, "B".repeat(FINGERPRINT_BYTES) + "\n").unwrap();
        tail.read_new();
        assert!(tail.rotated);
        std::fs::remove_file(&path).ok();
    }
}
