//! Acceptance tests for the definition of done in
//! docs/superpowers/specs/2026-08-06-poe2-resistance-calculator-design.md.
//!
//! Two criteria cannot be expressed here and are enforced elsewhere:
//!   - Criterion 7's "the exclusion survives a restart" depends on the settings
//!     store, in src-tauri/src/settings.rs and the poe2_set_item_excluded
//!     command; this crate has no settings.
//!   - Criterion 8, i18n labels, lives in src/i18n/locales/{en,ru}/translation.json
//!     and src/components/poe2/EquipmentTab.tsx.

#[cfg(test)]
mod tests {
    use crate::gear::resistances::Element;
    use crate::gear::slots::Slot;
    use crate::gear::summary::summarise;
    use crate::store::{StoredItem, StoredMod};
    use std::collections::BTreeMap;

    fn modifier(kind: &str, text: &str, value: f64) -> StoredMod {
        StoredMod {
            position: 0,
            effect_index: 0,
            kind: kind.to_string(),
            mod_name: None,
            tier: None,
            tags: Vec::new(),
            text: text.to_string(),
            value: Some(value),
            value_min: None,
            value_max: None,
        }
    }

    fn item(id: i64, class: &str, mods: Vec<StoredMod>) -> StoredItem {
        StoredItem {
            id,
            captured_ts: format!("2026-08-05T10:{:02}:00", id),
            raw_text: String::new(),
            source: "clipboard".to_string(),
            item_class: Some(class.to_string()),
            rarity: Some("Rare".to_string()),
            name: None,
            base_type: None,
            item_level: None,
            requires_level: None,
            quality: None,
            sockets: None,
            properties: BTreeMap::new(),
            requirements: BTreeMap::new(),
            advanced: true,
            mods,
        }
    }

    /// The player's nine captured items, carrying exactly the resistance
    /// modifiers their real gear carries. No belt — that absence is the point of
    /// the gap advice.
    fn real_gear() -> Vec<StoredItem> {
        vec![
            item(1, "Sceptres", vec![]),
            item(
                2,
                "Body Armours",
                vec![
                    modifier("rune", "+5% to all Elemental Resistances", 5.0),
                    modifier("rune", "+18% to Fire Resistance", 18.0),
                    modifier("suffix", "+18(16-20)% to Lightning Resistance", 18.0),
                    modifier("suffix", "+19(16-20)% to Cold Resistance", 19.0),
                ],
            ),
            item(
                3,
                "Helmets",
                vec![
                    modifier("rune", "+18% to Lightning Resistance", 18.0),
                    modifier("suffix", "+15(11-15)% to Lightning Resistance", 15.0),
                ],
            ),
            item(
                4,
                "Foci",
                vec![
                    modifier("rune", "+14% to Cold Resistance", 14.0),
                    modifier("rune", "+11% to Chaos Resistance", 11.0),
                    modifier("suffix", "+5(4-7)% to Chaos Resistance", 5.0),
                ],
            ),
            item(
                5,
                "Boots",
                vec![
                    modifier("rune", "+18% to Cold Resistance", 18.0),
                    modifier("suffix", "+28(26-30)% to Fire Resistance", 28.0),
                ],
            ),
            item(
                6,
                "Gloves",
                vec![
                    modifier("rune", "+18% to Fire Resistance", 18.0),
                    modifier("suffix", "+17(16-20)% to Lightning Resistance", 17.0),
                ],
            ),
            item(
                7,
                "Rings",
                vec![
                    modifier("implicit", "+30(20-30)% to Cold Resistance", 30.0),
                    modifier("suffix", "+9(9-11)% to all Elemental Resistances", 9.0),
                ],
            ),
            item(
                8,
                "Rings",
                vec![modifier("implicit", "+13(7-13)% to Chaos Resistance", 13.0)],
            ),
            item(
                9,
                "Amulets",
                vec![modifier("suffix", "+17(16-20)% to Fire Resistance", 17.0)],
            ),
        ]
    }

    fn total(summary: &crate::gear::summary::EquipmentSummary, element: Element) -> f64 {
        summary
            .lines
            .iter()
            .find(|l| l.element == element)
            .map(|l| l.from_gear)
            .expect("every element has a line")
    }

    /// Criterion 4: runes and implicits counted; the real nine items total
    /// 95 / 95 / 82 / 29.
    #[test]
    fn the_real_gear_totals_what_it_should() {
        let summary = summarise(&real_gear(), &[], None);
        assert_eq!(total(&summary, Element::Fire), 95.0);
        assert_eq!(total(&summary, Element::Cold), 95.0);
        assert_eq!(total(&summary, Element::Lightning), 82.0);
        assert_eq!(total(&summary, Element::Chaos), 29.0);
    }

    /// The +14 from two all-elemental sources is what makes fire and cold equal
    /// despite different single-element rolls — if all-elemental stopped being
    /// counted, this is the assertion that would notice.
    #[test]
    fn all_elemental_sources_contribute_fourteen() {
        let mut without = real_gear();
        for item in &mut without {
            item.mods.retain(|m| !m.text.contains("all Elemental"));
        }
        let summary = summarise(&without, &[], None);
        assert_eq!(total(&summary, Element::Fire), 95.0 - 14.0);
        assert_eq!(total(&summary, Element::Lightning), 82.0 - 14.0);
        assert_eq!(
            total(&summary, Element::Chaos),
            29.0,
            "chaos never took any of it"
        );
    }

    /// Criterion 2: with the penalty set, the figures match the character panel.
    #[test]
    fn the_penalty_reproduces_the_character_panel() {
        let summary = summarise(&real_gear(), &[], Some(24.0));
        let value = |element: Element| {
            summary
                .lines
                .iter()
                .find(|l| l.element == element)
                .and_then(|l| l.total)
                .unwrap()
        };
        assert_eq!(value(Element::Fire), 71.0);
        assert_eq!(value(Element::Cold), 71.0);
        assert_eq!(value(Element::Lightning), 58.0);
        assert_eq!(value(Element::Chaos), 29.0);
    }

    /// Criterion 3: without the penalty, no cap comparison is claimed for the
    /// elemental resistances.
    #[test]
    fn without_the_penalty_no_elemental_verdict_is_given() {
        let summary = summarise(&real_gear(), &[], None);
        for line in &summary.lines {
            if line.element == Element::Chaos {
                assert!(line.total.is_some(), "chaos takes no penalty");
            } else {
                assert!(line.total.is_none());
                assert!(line.short_by.is_none());
            }
        }
    }

    /// Criterion 6: the gaps name the belt, which this player has never captured,
    /// and the slots wearing nothing for lightning.
    #[test]
    fn the_gaps_name_the_missing_belt_and_the_silent_slots() {
        let summary = summarise(&real_gear(), &[], Some(24.0));
        let lightning = summary
            .lines
            .iter()
            .find(|l| l.element == Element::Lightning)
            .unwrap();

        assert_eq!(lightning.short_by, Some(17.0));
        assert!(
            lightning.empty_slots.contains(&Slot::Belt),
            "no belt was ever captured"
        );
        // The amulet and the second ring carry no lightning at all.
        assert!(lightning.missing_from.contains(&Slot::Amulet));
        // The helmet does carry lightning, so it is not named.
        assert!(!lightning.missing_from.contains(&Slot::Helmet));
    }

    /// Criterion 5: two rings are worn, nothing is superseded, nothing is
    /// unrecognised, and the belt is the one empty slot that matters.
    #[test]
    fn slot_inference_over_the_real_gear() {
        let summary = summarise(&real_gear(), &[], None);
        assert_eq!(summary.worn.len(), 9);
        assert!(summary.superseded.is_empty());
        assert!(summary.unrecognised.is_empty());
        assert!(summary.empty_slots.contains(&Slot::Belt));
        assert_eq!(
            summary
                .worn
                .iter()
                .filter(|(_, s)| *s == Slot::Ring)
                .count(),
            2
        );
    }

    /// Criterion 7's calculable half: excluding an item removes it from the
    /// totals. That the exclusion survives a restart is the settings store's job.
    #[test]
    fn excluding_the_boots_removes_their_contribution() {
        let summary = summarise(&real_gear(), &[5], Some(24.0));
        // The boots carried +28 fire and +18 cold.
        assert_eq!(total(&summary, Element::Fire), 95.0 - 28.0);
        assert_eq!(total(&summary, Element::Cold), 95.0 - 18.0);
        assert!(summary.empty_slots.contains(&Slot::Boots));
    }
}
