//! Which captured items count as worn.
//!
//! The program never sees the character's equipment — it sees what the player
//! copied. So worn gear is inferred: each item class maps to a slot, and within
//! a slot the most recently captured items fill it.

use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::BTreeMap;

use crate::store::StoredItem;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Type,
)]
#[serde(rename_all = "snake_case")]
pub enum Slot {
    Weapon,
    OffHand,
    BodyArmour,
    Helmet,
    Gloves,
    Boots,
    Belt,
    Amulet,
    Ring,
}

impl Slot {
    /// Rings are the only slot a character fills twice.
    pub fn capacity(&self) -> usize {
        match self {
            Slot::Ring => 2,
            _ => 1,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Slot::Weapon => "weapon",
            Slot::OffHand => "off_hand",
            Slot::BodyArmour => "body_armour",
            Slot::Helmet => "helmet",
            Slot::Gloves => "gloves",
            Slot::Boots => "boots",
            Slot::Belt => "belt",
            Slot::Amulet => "amulet",
            Slot::Ring => "ring",
        }
    }

    pub fn all() -> [Slot; 9] {
        [
            Slot::Weapon,
            Slot::OffHand,
            Slot::BodyArmour,
            Slot::Helmet,
            Slot::Gloves,
            Slot::Boots,
            Slot::Belt,
            Slot::Amulet,
            Slot::Ring,
        ]
    }
}

/// An item class the game writes in `Item Class:` -> the slot it occupies.
///
/// An unfamiliar class returns `None` rather than a guess: the game adds classes
/// every league, and silently filing a new one into the wrong slot would make the
/// totals wrong with no sign of it.
pub fn slot_for_class(item_class: &str) -> Option<Slot> {
    match item_class {
        "Body Armours" => Some(Slot::BodyArmour),
        "Helmets" => Some(Slot::Helmet),
        "Gloves" => Some(Slot::Gloves),
        "Boots" => Some(Slot::Boots),
        "Belts" => Some(Slot::Belt),
        "Amulets" => Some(Slot::Amulet),
        "Rings" => Some(Slot::Ring),
        "Foci" | "Shields" | "Quivers" => Some(Slot::OffHand),
        "Sceptres" | "Wands" | "Bows" | "Staves" | "Quarterstaves" | "One Hand Maces"
        | "Two Hand Maces" | "One Hand Swords" | "Two Hand Swords" | "One Hand Axes"
        | "Two Hand Axes" | "Crossbows" | "Spears" | "Flails" | "Daggers" | "Claws" => {
            Some(Slot::Weapon)
        }
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WornItem {
    pub item_id: i64,
    pub slot: Slot,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Worn {
    pub worn: Vec<WornItem>,
    /// Displaced by something newer in the same slot. Kept rather than dropped so
    /// the interface can say why an item is not counted.
    pub superseded: Vec<i64>,
    /// A class that maps to no slot — a flask, a gem, currency, or something the
    /// game added since this was written.
    pub unrecognised: Vec<i64>,
}

/// Works out which captured items are being worn.
///
/// Sorts by capture time itself rather than trusting the caller's order, so the
/// "most recent wins" rule cannot be broken by passing an unsorted list.
pub fn infer_worn(items: &[StoredItem], excluded: &[i64]) -> Worn {
    let mut candidates: Vec<&StoredItem> = items
        .iter()
        .filter(|item| !excluded.contains(&item.id))
        .collect();
    // Newest first; ties broken by id descending so the result is deterministic
    // when two items carry the same timestamp.
    candidates.sort_by(|a, b| {
        b.captured_ts
            .cmp(&a.captured_ts)
            .then_with(|| b.id.cmp(&a.id))
    });

    let mut result = Worn::default();
    let mut filled: BTreeMap<Slot, usize> = BTreeMap::new();

    for item in candidates {
        let Some(class) = item.item_class.as_deref() else {
            result.unrecognised.push(item.id);
            continue;
        };
        let Some(slot) = slot_for_class(class) else {
            result.unrecognised.push(item.id);
            continue;
        };

        let taken = filled.entry(slot).or_insert(0);
        if *taken < slot.capacity() {
            *taken += 1;
            result.worn.push(WornItem {
                item_id: item.id,
                slot,
            });
        } else {
            result.superseded.push(item.id);
        }
    }

    result.superseded.sort();
    result.unrecognised.sort();
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::StoredItem;
    use std::collections::BTreeMap;

    fn item(id: i64, class: &str, ts: &str) -> StoredItem {
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
            mods: Vec::new(),
        }
    }

    fn worn_ids(w: &Worn) -> Vec<i64> {
        let mut ids: Vec<i64> = w.worn.iter().map(|x| x.item_id).collect();
        ids.sort();
        ids
    }

    #[test]
    fn known_classes_map_to_slots() {
        assert_eq!(slot_for_class("Body Armours"), Some(Slot::BodyArmour));
        assert_eq!(slot_for_class("Helmets"), Some(Slot::Helmet));
        assert_eq!(slot_for_class("Gloves"), Some(Slot::Gloves));
        assert_eq!(slot_for_class("Boots"), Some(Slot::Boots));
        assert_eq!(slot_for_class("Belts"), Some(Slot::Belt));
        assert_eq!(slot_for_class("Amulets"), Some(Slot::Amulet));
        assert_eq!(slot_for_class("Rings"), Some(Slot::Ring));
        assert_eq!(slot_for_class("Sceptres"), Some(Slot::Weapon));
        assert_eq!(slot_for_class("Bows"), Some(Slot::Weapon));
        assert_eq!(slot_for_class("Foci"), Some(Slot::OffHand));
        assert_eq!(slot_for_class("Shields"), Some(Slot::OffHand));
    }

    #[test]
    fn unknown_classes_map_to_nothing() {
        // The game adds classes every league; an unknown one must not be
        // guessed into a slot.
        assert_eq!(slot_for_class("Life Flasks"), None);
        assert_eq!(slot_for_class("Skill Gems"), None);
        assert_eq!(slot_for_class("Stackable Currency"), None);
        assert_eq!(slot_for_class(""), None);
    }

    #[test]
    fn rings_hold_two_everything_else_one() {
        assert_eq!(Slot::Ring.capacity(), 2);
        for slot in Slot::all() {
            if slot != Slot::Ring {
                assert_eq!(slot.capacity(), 1, "{slot:?} should hold one item");
            }
        }
    }

    #[test]
    fn a_single_item_per_slot_is_all_worn() {
        let items = vec![
            item(1, "Body Armours", "2026-08-05T10:00:00"),
            item(2, "Helmets", "2026-08-05T10:01:00"),
        ];
        let w = infer_worn(&items, &[]);
        assert_eq!(worn_ids(&w), vec![1, 2]);
        assert!(w.superseded.is_empty());
        assert!(w.unrecognised.is_empty());
    }

    #[test]
    fn the_most_recent_wins_a_contested_slot() {
        let items = vec![
            item(1, "Helmets", "2026-08-05T10:00:00"),
            item(2, "Helmets", "2026-08-06T09:00:00"),
        ];
        let w = infer_worn(&items, &[]);
        assert_eq!(worn_ids(&w), vec![2]);
        assert_eq!(w.superseded, vec![1]);
    }

    #[test]
    fn order_of_input_does_not_matter() {
        // The caller must not be able to break the rule by passing an unsorted list.
        let newest_first = vec![
            item(2, "Helmets", "2026-08-06T09:00:00"),
            item(1, "Helmets", "2026-08-05T10:00:00"),
        ];
        assert_eq!(worn_ids(&infer_worn(&newest_first, &[])), vec![2]);
    }

    #[test]
    fn rings_keep_the_two_most_recent() {
        let items = vec![
            item(1, "Rings", "2026-08-04T10:00:00"),
            item(2, "Rings", "2026-08-05T10:00:00"),
            item(3, "Rings", "2026-08-06T10:00:00"),
        ];
        let w = infer_worn(&items, &[]);
        assert_eq!(worn_ids(&w), vec![2, 3]);
        assert_eq!(w.superseded, vec![1]);
    }

    #[test]
    fn a_weapon_and_an_off_hand_do_not_collide() {
        let items = vec![
            item(1, "Sceptres", "2026-08-05T10:00:00"),
            item(2, "Foci", "2026-08-05T10:01:00"),
        ];
        assert_eq!(worn_ids(&infer_worn(&items, &[])), vec![1, 2]);
    }

    #[test]
    fn unrecognised_classes_are_reported_not_dropped() {
        let items = vec![
            item(1, "Body Armours", "2026-08-05T10:00:00"),
            item(2, "Life Flasks", "2026-08-05T10:01:00"),
        ];
        let w = infer_worn(&items, &[]);
        assert_eq!(worn_ids(&w), vec![1]);
        assert_eq!(w.unrecognised, vec![2]);
        assert!(w.superseded.is_empty());
    }

    #[test]
    fn an_item_with_no_class_is_unrecognised() {
        let mut orphan = item(1, "Body Armours", "2026-08-05T10:00:00");
        orphan.item_class = None;
        let w = infer_worn(&[orphan], &[]);
        assert!(w.worn.is_empty());
        assert_eq!(w.unrecognised, vec![1]);
    }

    #[test]
    fn excluded_items_are_neither_worn_nor_superseded() {
        let items = vec![
            item(1, "Helmets", "2026-08-05T10:00:00"),
            item(2, "Helmets", "2026-08-06T09:00:00"),
        ];
        // Excluding the newest promotes the older one rather than emptying the slot.
        let w = infer_worn(&items, &[2]);
        assert_eq!(worn_ids(&w), vec![1]);
        assert!(w.superseded.is_empty());
    }

    #[test]
    fn no_items_is_not_an_error() {
        let w = infer_worn(&[], &[]);
        assert!(w.worn.is_empty());
        assert!(w.superseded.is_empty());
        assert!(w.unrecognised.is_empty());
    }
}
