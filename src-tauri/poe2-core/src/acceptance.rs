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
    ///
    /// A bare count of modifiers would pass even if reparsing scrambled their
    /// structure — a prefix relabelled as a suffix, a tier dropped, a rune
    /// duplicated in place of a missing affix. So this compares the reparsed,
    /// stored structure against a fresh `parse_item` of the same raw text,
    /// field by field and in order: that is what "reparses stored items"
    /// actually promises.
    #[test]
    fn rebuild_reparses_stored_items() {
        let mut store = Poe2Store::in_memory().unwrap();
        let parsed = parse_item(BODY_ARMOUR).unwrap();
        let (id, _) = store.add_item(&parsed, "paste", Utc::now()).unwrap();

        let raw = store.raw_items().unwrap();
        assert_eq!(raw.len(), 1);
        let reparsed = parse_item(&raw[0].1).unwrap();
        store.reparse_item(id, &reparsed).unwrap();

        let stored_mods = store.item(id).unwrap().unwrap().mods;
        assert_eq!(stored_mods.len(), 5);
        assert_eq!(reparsed.mods.len(), 5);

        // A row is one effect; effects of the same modifier share a position.
        // Compare the first effect row at each position against the matching
        // freshly parsed modifier, in order: kind, name, tier and first value.
        for (position, expected) in reparsed.mods.iter().enumerate() {
            let actual_first_effect = stored_mods
                .iter()
                .find(|m| m.position == position as i64 && m.effect_index == 0)
                .unwrap_or_else(|| panic!("no stored modifier at position {position}"));

            assert_eq!(
                actual_first_effect.kind,
                expected.kind.as_str(),
                "kind mismatch at position {position}"
            );
            assert_eq!(
                actual_first_effect.mod_name, expected.name,
                "name mismatch at position {position}"
            );
            assert_eq!(
                actual_first_effect.tier, expected.tier,
                "tier mismatch at position {position}"
            );
            assert_eq!(
                actual_first_effect.value,
                expected.effects[0].values.first().map(|v| v.value),
                "first value mismatch at position {position}"
            );
        }
    }

    /// Criterion 7: runes are stored apart from affixes, not just summed.
    ///
    /// Summing `ModKind::Rune` values alone would not catch a misclassification:
    /// the body armour's suffix `of the Storm` is +18% Lightning Resistance,
    /// the same magnitude as the `+18% to Fire Resistance` rune, so a bug that
    /// tagged the suffix as a rune and dropped the fire rune would still sum to
    /// 23.0. So this asserts on the identity of the rune set — its exact texts,
    /// that neither carries a name or tier, and that no affix text leaks into
    /// it — not only on a total.
    #[test]
    fn runes_contribute_resistances() {
        let item = parse_item(BODY_ARMOUR).unwrap();
        let runes: Vec<_> = item
            .mods
            .iter()
            .filter(|m| m.kind == ModKind::Rune)
            .collect();

        let mut rune_texts: Vec<&str> = runes.iter().map(|m| m.effects[0].text.as_str()).collect();
        rune_texts.sort();
        assert_eq!(
            rune_texts,
            vec![
                "+18% to Fire Resistance",
                "+5% to all Elemental Resistances"
            ]
        );
        assert!(runes.iter().all(|m| m.name.is_none() && m.tier.is_none()));

        let affix_texts: Vec<&str> = item
            .mods
            .iter()
            .filter(|m| m.kind != ModKind::Rune)
            .flat_map(|m| m.effects.iter().map(|e| e.text.as_str()))
            .collect();
        assert!(rune_texts.iter().all(|t| !affix_texts.contains(t)));

        let total: f64 = runes.iter().map(|m| m.effects[0].values[0].value).sum();
        assert_eq!(total, 23.0);
    }
}
