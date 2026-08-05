//! Totals, the campaign penalty, and where the missing points could come from.

use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::{BTreeMap, BTreeSet};

use crate::gear::resistances::{resistance_from_mod, Element};
use crate::gear::slots::{infer_worn, Slot};
use crate::store::StoredItem;

/// Every resistance caps at 75. Modifiers that raise a maximum are out of scope,
/// and the player has none.
pub const CAP: f64 = 75.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct ResistanceLine {
    pub element: Element,
    /// What the worn gear adds up to, before the campaign penalty.
    pub from_gear: f64,
    /// The figure the character panel would show. `None` for the three elemental
    /// resistances while the penalty is unknown — neither "met" nor "short" can
    /// be claimed without it.
    pub total: Option<f64>,
    pub cap: f64,
    pub short_by: Option<f64>,
    /// Worn slots contributing nothing to this element.
    pub missing_from: Vec<Slot>,
    /// Slots with nothing captured for them at all.
    pub empty_slots: Vec<Slot>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct EquipmentSummary {
    pub lines: Vec<ResistanceLine>,
    pub penalty: Option<f64>,
    /// Available to Rust callers (and tests) to work out per-item status, but not
    /// read by the frontend — `poe2_equipment` already folds this into `items`.
    #[serde(skip)]
    pub worn: Vec<(i64, Slot)>,
    #[serde(skip)]
    pub superseded: Vec<i64>,
    #[serde(skip)]
    pub unrecognised: Vec<i64>,
    pub empty_slots: Vec<Slot>,
}

pub fn summarise(items: &[StoredItem], excluded: &[i64], penalty: Option<f64>) -> EquipmentSummary {
    let worn = infer_worn(items, excluded);
    let by_id: BTreeMap<i64, &StoredItem> = items.iter().map(|item| (item.id, item)).collect();

    let filled: BTreeSet<Slot> = worn.worn.iter().map(|w| w.slot).collect();
    let empty_slots: Vec<Slot> = Slot::all()
        .into_iter()
        .filter(|slot| !filled.contains(slot))
        .collect();

    let mut totals: BTreeMap<Element, f64> = Element::all().into_iter().map(|e| (e, 0.0)).collect();
    // Which slots give something to each element, so the gaps can name the ones
    // that do not.
    let mut contributing: BTreeMap<Element, BTreeSet<Slot>> = BTreeMap::new();

    for worn_item in &worn.worn {
        let Some(item) = by_id.get(&worn_item.item_id) else {
            continue;
        };
        for modifier in &item.mods {
            for (element, value) in resistance_from_mod(modifier) {
                *totals.entry(element).or_insert(0.0) += value;
                contributing
                    .entry(element)
                    .or_default()
                    .insert(worn_item.slot);
            }
        }
    }

    let lines = Element::all()
        .into_iter()
        .map(|element| {
            let from_gear = totals.get(&element).copied().unwrap_or(0.0);
            let is_elemental = Element::elemental().contains(&element);

            // Chaos takes no penalty, so its total is known whether or not the
            // player has supplied one.
            let total = if is_elemental {
                penalty.map(|p| (from_gear - p).max(0.0))
            } else {
                Some(from_gear)
            };

            let short_by = total.and_then(|t| {
                let gap = CAP - t;
                (gap > 0.0).then_some(gap)
            });

            let (missing_from, element_empty) = if short_by.is_some() {
                let gives = contributing.get(&element).cloned().unwrap_or_default();
                (
                    filled
                        .iter()
                        .copied()
                        .filter(|slot| !gives.contains(slot))
                        .collect(),
                    empty_slots.clone(),
                )
            } else {
                (Vec::new(), Vec::new())
            };

            ResistanceLine {
                element,
                from_gear,
                total,
                cap: CAP,
                short_by,
                missing_from,
                empty_slots: element_empty,
            }
        })
        .collect();

    EquipmentSummary {
        lines,
        penalty,
        worn: worn.worn.iter().map(|w| (w.item_id, w.slot)).collect(),
        superseded: worn.superseded,
        unrecognised: worn.unrecognised,
        empty_slots,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gear::resistances::Element;
    use crate::gear::slots::Slot;
    use crate::store::{StoredItem, StoredMod};
    use std::collections::BTreeMap;

    fn modifier(text: &str, value: f64) -> StoredMod {
        StoredMod {
            position: 0,
            effect_index: 0,
            kind: "suffix".to_string(),
            mod_name: None,
            tier: None,
            tags: Vec::new(),
            text: text.to_string(),
            value: Some(value),
            value_min: None,
            value_max: None,
        }
    }

    fn item(id: i64, class: &str, ts: &str, mods: Vec<StoredMod>) -> StoredItem {
        StoredItem {
            id,
            captured_ts: ts.to_string(),
            raw_text: String::new(),
            source: "paste".to_string(),
            item_class: Some(class.to_string()),
            rarity: None,
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

    fn line(summary: &EquipmentSummary, element: Element) -> ResistanceLine {
        summary
            .lines
            .iter()
            .find(|l| l.element == element)
            .cloned()
            .expect("every element has a line")
    }

    #[test]
    fn every_element_gets_a_line_even_with_no_items() {
        let summary = summarise(&[], &[], None);
        assert_eq!(summary.lines.len(), 4);
        assert_eq!(line(&summary, Element::Fire).from_gear, 0.0);
    }

    #[test]
    fn gear_totals_add_up_across_items_and_kinds() {
        let mut rune = modifier("+18% to Fire Resistance", 18.0);
        rune.kind = "rune".to_string();
        let mut implicit = modifier("+30% to Cold Resistance", 30.0);
        implicit.kind = "implicit".to_string();

        let items = vec![
            item(1, "Body Armours", "2026-08-05T10:00:00", vec![rune]),
            item(2, "Rings", "2026-08-05T10:01:00", vec![implicit]),
            item(
                3,
                "Boots",
                "2026-08-05T10:02:00",
                vec![modifier("+28% to Fire Resistance", 28.0)],
            ),
        ];
        let summary = summarise(&items, &[], None);
        assert_eq!(line(&summary, Element::Fire).from_gear, 46.0);
        assert_eq!(line(&summary, Element::Cold).from_gear, 30.0);
    }

    #[test]
    fn all_elemental_lifts_three_and_leaves_chaos() {
        let items = vec![item(
            1,
            "Rings",
            "2026-08-05T10:00:00",
            vec![modifier("+9% to all Elemental Resistances", 9.0)],
        )];
        let summary = summarise(&items, &[], None);
        assert_eq!(line(&summary, Element::Fire).from_gear, 9.0);
        assert_eq!(line(&summary, Element::Cold).from_gear, 9.0);
        assert_eq!(line(&summary, Element::Lightning).from_gear, 9.0);
        assert_eq!(line(&summary, Element::Chaos).from_gear, 0.0);
    }

    #[test]
    fn superseded_items_do_not_count() {
        let items = vec![
            item(
                1,
                "Helmets",
                "2026-08-05T10:00:00",
                vec![modifier("+40% to Fire Resistance", 40.0)],
            ),
            item(
                2,
                "Helmets",
                "2026-08-06T10:00:00",
                vec![modifier("+10% to Fire Resistance", 10.0)],
            ),
        ];
        let summary = summarise(&items, &[], None);
        assert_eq!(line(&summary, Element::Fire).from_gear, 10.0);
        assert_eq!(summary.superseded, vec![1]);
    }

    #[test]
    fn excluded_items_do_not_count() {
        let items = vec![item(
            1,
            "Helmets",
            "2026-08-05T10:00:00",
            vec![modifier("+40% to Fire Resistance", 40.0)],
        )];
        assert_eq!(summarise(&items, &[1], None).lines[0].from_gear, 0.0);
    }

    #[test]
    fn without_a_penalty_no_cap_comparison_is_offered() {
        // Neither "met" nor "short" is known, so neither is claimed.
        let items = vec![item(
            1,
            "Boots",
            "2026-08-05T10:00:00",
            vec![modifier("+80% to Fire Resistance", 80.0)],
        )];
        let fire = line(&summarise(&items, &[], None), Element::Fire);
        assert_eq!(fire.from_gear, 80.0);
        assert_eq!(fire.total, None);
        assert_eq!(fire.short_by, None);
    }

    #[test]
    fn the_penalty_applies_to_the_three_elements() {
        let items = vec![item(
            1,
            "Boots",
            "2026-08-05T10:00:00",
            vec![modifier("+95% to all Elemental Resistances", 95.0)],
        )];
        let summary = summarise(&items, &[], Some(24.0));
        for element in Element::elemental() {
            let l = line(&summary, element);
            assert_eq!(l.from_gear, 95.0);
            assert_eq!(l.total, Some(71.0));
            assert_eq!(l.short_by, Some(4.0));
        }
    }

    #[test]
    fn the_penalty_never_applies_to_chaos() {
        let items = vec![item(
            1,
            "Rings",
            "2026-08-05T10:00:00",
            vec![modifier("+29% to Chaos Resistance", 29.0)],
        )];
        let chaos = line(&summarise(&items, &[], Some(24.0)), Element::Chaos);
        assert_eq!(chaos.total, Some(29.0));
        assert_eq!(chaos.short_by, Some(46.0));
    }

    #[test]
    fn chaos_is_compared_to_the_cap_even_without_a_penalty() {
        let items = vec![item(
            1,
            "Rings",
            "2026-08-05T10:00:00",
            vec![modifier("+29% to Chaos Resistance", 29.0)],
        )];
        let chaos = line(&summarise(&items, &[], None), Element::Chaos);
        assert_eq!(chaos.total, Some(29.0));
        assert_eq!(chaos.short_by, Some(46.0));
    }

    #[test]
    fn a_resistance_at_or_over_the_cap_is_not_short() {
        let items = vec![item(
            1,
            "Boots",
            "2026-08-05T10:00:00",
            vec![modifier("+99% to Fire Resistance", 99.0)],
        )];
        let fire = line(&summarise(&items, &[], Some(24.0)), Element::Fire);
        assert_eq!(fire.total, Some(75.0));
        assert_eq!(fire.short_by, None);
    }

    #[test]
    fn the_total_never_goes_below_zero() {
        let items = vec![item(
            1,
            "Boots",
            "2026-08-05T10:00:00",
            vec![modifier("+10% to Fire Resistance", 10.0)],
        )];
        let fire = line(&summarise(&items, &[], Some(60.0)), Element::Fire);
        assert_eq!(
            fire.total,
            Some(0.0),
            "a negative resistance is not a thing here"
        );
    }

    #[test]
    fn gaps_name_worn_items_giving_nothing_and_slots_standing_empty() {
        let items = vec![
            item(
                1,
                "Boots",
                "2026-08-05T10:00:00",
                vec![modifier("+10% to Fire Resistance", 10.0)],
            ),
            item(2, "Rings", "2026-08-05T10:01:00", vec![]),
        ];
        let fire = line(&summarise(&items, &[], Some(0.0)), Element::Fire);
        // The ring contributes nothing to fire, so it is named.
        assert!(fire.missing_from.contains(&Slot::Ring));
        // Boots do contribute, so they are not.
        assert!(!fire.missing_from.contains(&Slot::Boots));
        // Nothing was captured for these at all.
        assert!(fire.empty_slots.contains(&Slot::Belt));
        assert!(fire.empty_slots.contains(&Slot::Helmet));
    }

    #[test]
    fn a_resistance_at_the_cap_names_no_gaps() {
        let items = vec![item(
            1,
            "Boots",
            "2026-08-05T10:00:00",
            vec![modifier("+80% to Fire Resistance", 80.0)],
        )];
        let fire = line(&summarise(&items, &[], Some(0.0)), Element::Fire);
        assert!(fire.missing_from.is_empty());
        assert!(fire.empty_slots.is_empty());
    }

    #[test]
    fn empty_slots_are_reported_once_at_the_top_level() {
        let items = vec![item(1, "Boots", "2026-08-05T10:00:00", vec![])];
        let summary = summarise(&items, &[], None);
        assert!(summary.empty_slots.contains(&Slot::Belt));
        assert!(!summary.empty_slots.contains(&Slot::Boots));
    }
}
