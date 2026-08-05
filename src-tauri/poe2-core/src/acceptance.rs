//! Acceptance tests for the definition of done in
//! docs/superpowers/specs/2026-08-05-poe2-items-design.md.
//!
//! Two criteria from that definition cannot be expressed as tests in this
//! crate, because they live in Tauri-side code and the UI:
//!
//! - Criterion 5's "off by default": enforced by `default_poe2_clipboard_watch`
//!   in `src-tauri/src/settings.rs`, which returns `false`. The rest of
//!   criterion 5 (Item Class gate, never writes) IS covered here, indirectly,
//!   by the tests in `clipboard_watch.rs`.
//! - Criterion 8 (section hidden until enabled, labels from i18n): enforced by
//!   the sidebar gate and locale files in `src/components/Sidebar.tsx` and the
//!   i18n resources.

#[cfg(test)]
mod tests {
    use crate::items::{parse_item, ModKind};
    use crate::store::Poe2Store;
    use chrono::Utc;

    const SCEPTRE: &str = include_str!("fixtures/sceptre.txt");
    const BODY_ARMOUR: &str = include_str!("fixtures/body_armour.txt");

    /// Criterion 1: the sceptre parses completely.
    #[test]
    fn sceptre_is_parsed_completely() {
        let item = parse_item(SCEPTRE).unwrap();
        assert_eq!(item.item_class.as_deref(), Some("Sceptres"));
        assert_eq!(item.rarity.as_deref(), Some("Rare"));
        assert_eq!(item.name.as_deref(), Some("Wrath Call"));
        assert_eq!(item.base_type.as_deref(), Some("Rattling Sceptre"));
        assert_eq!(item.item_level, Some(58));
        assert_eq!(item.requires_level, Some(44));
        assert_eq!(item.quality, Some(20));
        assert_eq!(item.mods.len(), 4);
        assert!(item
            .mods
            .iter()
            .all(|m| matches!(m.kind, ModKind::Prefix | ModKind::Suffix | ModKind::Crafted)));
        let tiers: Vec<Option<i64>> = item.mods.iter().map(|m| m.tier).collect();
        assert_eq!(tiers, vec![Some(4), Some(6), Some(2), None]);
        assert_eq!(
            item.mods[2].tags,
            vec!["Minion".to_string(), "Gem".to_string()]
        );
        assert_eq!(item.mods[0].effects[0].values[0].value_max, Some(50.0));
    }

    /// Criterion 2: Counselor's is one modifier with two effects.
    #[test]
    fn multiline_mod_stays_one_mod() {
        let item = parse_item(SCEPTRE).unwrap();
        let counselors: Vec<_> = item
            .mods
            .iter()
            .filter(|m| m.name.as_deref() == Some("Counselor's"))
            .collect();
        assert_eq!(counselors.len(), 1);
        assert_eq!(counselors[0].effects.len(), 2);
    }

    /// Criterion 3: an item pasted twice is stored once.
    #[test]
    fn item_pasted_twice_is_stored_once() {
        let mut store = Poe2Store::in_memory().unwrap();
        let parsed = parse_item(SCEPTRE).unwrap();
        store.add_item(&parsed, "paste", Utc::now()).unwrap();
        store.add_item(&parsed, "paste", Utc::now()).unwrap();
        assert_eq!(store.items(50).unwrap().len(), 1);
    }

    /// Criterion 4: text without Item Class stores nothing.
    #[test]
    fn text_without_item_class_is_rejected() {
        assert!(parse_item("my password").is_err());
    }

    /// Criterion 6: rebuilding reparses stored items from raw text.
    #[test]
    fn rebuild_reparses_stored_items() {
        let mut store = Poe2Store::in_memory().unwrap();
        let parsed = parse_item(BODY_ARMOUR).unwrap();
        let (id, _) = store.add_item(&parsed, "paste", Utc::now()).unwrap();

        let raw = store.raw_items().unwrap();
        assert_eq!(raw.len(), 1);
        let reparsed = parse_item(&raw[0].1).unwrap();
        store.reparse_item(id, &reparsed).unwrap();

        assert_eq!(store.item(id).unwrap().unwrap().mods.len(), 5);
    }

    /// Criterion 7: runes are stored apart and carry their values.
    #[test]
    fn runes_contribute_resistances() {
        let item = parse_item(BODY_ARMOUR).unwrap();
        let total: f64 = item
            .mods
            .iter()
            .filter(|m| m.kind == ModKind::Rune)
            .map(|m| m.effects[0].values[0].value)
            .sum();
        assert_eq!(total, 23.0);
    }
}
