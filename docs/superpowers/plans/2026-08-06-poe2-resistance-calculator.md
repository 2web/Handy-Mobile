# Path of Exile 2 Resistance Calculator Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** An "Equipment" tab that sums what the player's captured gear contributes to each resistance, compares it against the cap once the player supplies the campaign penalty, and names the slots where the missing points could come from.

**Architecture:** A new `gear` module in the existing `poe2-core` crate holds all of it as pure functions over already-stored items: slot inference, resistance extraction, the penalty, and the gap advice. The `handy` crate adds two commands and one setting. The React side adds a third tab. Nothing new is stored — the calculation runs over `Poe2Store::items` on demand.

**Tech Stack:** Rust 2021, `regex`, `once_cell`, `serde`, `specta`, `rusqlite`, Tauri 2.11. Frontend: React + TypeScript, i18next.

## Global Constraints

- **No new third-party crates or npm packages.**
- `poe2-core` contains **no Tauri types**; the `gear` module performs no I/O and no database calls — it takes already-loaded items as input.
- **Resistances only.** No life, energy shield, armour, evasion, spirit or damage anywhere in this feature.
- **The penalty is never guessed.** While `poe2_resistance_penalty` is unset, no cap comparison is shown — not "met", not "short". A number that is confidently wrong in the reassuring direction is worse than no number, and in a hardcore league it costs the character.
- The penalty applies to **fire, cold and lightning, never to chaos**.
- **The cap is 75** for every resistance in this feature; modifiers that raise a maximum are out of scope.
- Nothing an item says becomes markup in the UI — game text reaches the DOM only as React text children.
- Only these pre-existing files may be edited: `src-tauri/poe2-core/src/lib.rs`, `src-tauri/src/poe2/commands.rs`, `src-tauri/src/settings.rs`, `src-tauri/src/lib.rs`, `src/components/poe2/Poe2Page.tsx`, `src/stores/settingsStore.ts`, `src/i18n/locales/en/translation.json`, `src/i18n/locales/ru/translation.json`. Everything else is new.
- i18n keys in **`en` and `ru` only**; this fork's other 22 locales are deliberately behind and `bun scripts/check-translations.ts` already fails on them.
- **Tests run in `poe2-core`:** `cd src-tauri/poe2-core && cargo test`, seconds. Never `cargo test` in `src-tauri` — that binary cannot start on this machine (`STATUS_ENTRYPOINT_NOT_FOUND`, from `transcribe-cpp`'s native DLLs). Use `cd src-tauri && cargo build` to check the `handy` crate compiles.
- Format with `cargo fmt` in both crates, then `git status` and revert unrelated files the formatter touched — this repo has pre-existing drift in `actions.rs` and `tray.rs`.
- Frontend gates: `bun run lint` and `bunx tsc --noEmit`, both clean.
- Commit messages in English, `feat(poe2):` / `fix(poe2):` / `test(poe2):` style. No Co-Authored-By trailer.

---

## What already exists — use as is

`src-tauri/poe2-core/src/lib.rs` — `pub mod clipboard_watch; pub mod items; pub mod log; pub mod store;` plus `#[cfg(test)] mod acceptance;`. 122 tests pass.

`src-tauri/poe2-core/src/store.rs` — `Poe2Store` with `items(&self, limit: i64) -> anyhow::Result<Vec<StoredItem>>`, and:

```rust
pub struct StoredItem {
    pub id: i64,
    pub captured_ts: String,      // RFC3339
    pub raw_text: String,
    pub source: String,
    pub item_class: Option<String>,
    pub rarity: Option<String>,
    pub name: Option<String>,
    pub base_type: Option<String>,
    pub item_level: Option<i64>,
    pub requires_level: Option<i64>,
    pub quality: Option<i64>,
    pub sockets: Option<String>,
    pub properties: BTreeMap<String, String>,
    pub requirements: BTreeMap<String, i64>,
    pub advanced: bool,
    pub mods: Vec<StoredMod>,
}

pub struct StoredMod {
    pub position: i64,
    pub effect_index: i64,
    pub kind: String,             // "prefix" | "suffix" | "implicit" | "crafted" | "rune" | "unknown"
    pub mod_name: Option<String>,
    pub tier: Option<i64>,
    pub tags: Vec<String>,
    pub text: String,             // e.g. "+17(16-20)% to Fire Resistance"
    pub value: Option<f64>,       // the rolled value, 17.0 above
    pub value_min: Option<f64>,
    pub value_max: Option<f64>,
}
```

`src-tauri/src/poe2/commands.rs` — `store_for(app: &AppHandle) -> Result<Poe2Store, String>`, the item and progress commands, and the `change_*_setting` family.

`src-tauri/src/settings.rs` — `AppSettings` with `#[serde(default = "…")]` per field and a struct literal in `get_default_settings()` that must also list every field.

`src/components/poe2/Poe2Page.tsx` — a `Tab` union of `"progress" | "items"`, a button row mapping over it, and `{tab === "progress" ? <ProgressTab /> : <ItemsPage />}`.

`src/stores/settingsStore.ts` — the `settingUpdaters` map. **A key missing there silently fails to persist and only logs to the console.**

**The player's real data**, which the acceptance test in Task 6 asserts against: nine items — a sceptre, body armour, helmet, focus, boots, gloves, two rings, an amulet, no belt — totalling fire 95, cold 95, lightning 82, chaos 29, including +14 from two all-elemental sources (a `+5%` rune and a `+9%` suffix).

---

## File structure

| File | Responsibility |
|---|---|
| `src-tauri/poe2-core/src/gear/mod.rs` | module wiring |
| `src-tauri/poe2-core/src/gear/slots.rs` | item class → slot, and which captured items are worn |
| `src-tauri/poe2-core/src/gear/resistances.rs` | pulling resistance values out of modifier text |
| `src-tauri/poe2-core/src/gear/summary.rs` | totals, the penalty, the cap, and the gap advice |
| `src-tauri/src/poe2/commands.rs` | `poe2_equipment`, `poe2_set_item_excluded` |
| `src/components/poe2/EquipmentTab.tsx` | the tab |

---

## Task 1: Slots and what counts as worn

**Files:**
- Create: `src-tauri/poe2-core/src/gear/mod.rs`, `src-tauri/poe2-core/src/gear/slots.rs`
- Modify: `src-tauri/poe2-core/src/lib.rs`

**Interfaces:**
- Consumes: `crate::store::StoredItem`.
- Produces:
  - `gear::slots::Slot` — enum `Weapon`, `OffHand`, `BodyArmour`, `Helmet`, `Gloves`, `Boots`, `Belt`, `Amulet`, `Ring`; derives `Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, specta::Type`; `#[serde(rename_all = "snake_case")]`; methods `capacity(&self) -> usize` (2 for `Ring`, else 1), `as_str(&self) -> &'static str`, and `all() -> [Slot; 9]`
  - `gear::slots::slot_for_class(item_class: &str) -> Option<Slot>`
  - `gear::slots::WornItem { pub item_id: i64, pub slot: Slot }`
  - `gear::slots::Worn { pub worn: Vec<WornItem>, pub superseded: Vec<i64>, pub unrecognised: Vec<i64> }`
  - `gear::slots::infer_worn(items: &[StoredItem], excluded: &[i64]) -> Worn`

`infer_worn` takes items in any order and sorts by `captured_ts` descending itself, so callers cannot get the "most recent wins" rule wrong by passing an unsorted list.

- [ ] **Step 1: Write the failing tests**

Create `src-tauri/poe2-core/src/gear/slots.rs` containing only this test module for now:

```rust
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
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cd src-tauri/poe2-core && cargo test gear::
```

Expected: compilation error — `file not found for module gear`.

- [ ] **Step 3: Write the implementation**

Create `src-tauri/poe2-core/src/gear/mod.rs`:

```rust
//! What the player's captured gear adds up to.
//!
//! Pure functions over items already loaded from the store: nothing here reads
//! the database or the filesystem.

pub mod slots;
```

Insert above the `#[cfg(test)]` block in `src-tauri/poe2-core/src/gear/slots.rs`:

```rust
//! Which captured items count as worn.
//!
//! The program never sees the character's equipment — it sees what the player
//! copied. So worn gear is inferred: each item class maps to a slot, and within
//! a slot the most recently captured items fill it.

use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::BTreeMap;

use crate::store::StoredItem;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Type)]
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
```

Add `pub mod gear;` to `src-tauri/poe2-core/src/lib.rs`, beside the existing `pub mod items;`.

- [ ] **Step 4: Run the tests to verify they pass**

```bash
cd src-tauri/poe2-core && cargo test
```

Expected: PASS — the 122 existing tests plus 11 new ones.

- [ ] **Step 5: Format and commit**

```bash
cd src-tauri/poe2-core && cargo fmt
cd ../.. && git add src-tauri/poe2-core
git commit -m "feat(poe2): infer which captured items are worn, by slot"
```

---

## Task 2: Reading resistances out of modifier text

**Files:**
- Create: `src-tauri/poe2-core/src/gear/resistances.rs`
- Modify: `src-tauri/poe2-core/src/gear/mod.rs`

**Interfaces:**
- Consumes: `crate::store::StoredMod`.
- Produces:
  - `gear::resistances::Element` — enum `Fire`, `Cold`, `Lightning`, `Chaos`; derives `Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, specta::Type`; `#[serde(rename_all = "snake_case")]`; `as_str(&self) -> &'static str`; `all() -> [Element; 4]`; `elemental() -> [Element; 3]` (fire, cold, lightning — the three the penalty touches)
  - `gear::resistances::resistance_from_mod(m: &StoredMod) -> Vec<(Element, f64)>`

A modifier yields a list because one line can feed several elements: `+9% to all Elemental Resistances` gives three, and the two-element shape gives two.

- [ ] **Step 1: Write the failing tests**

Create `src-tauri/poe2-core/src/gear/resistances.rs` with only this test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::StoredMod;

    fn m(text: &str, value: Option<f64>) -> StoredMod {
        StoredMod {
            position: 0,
            effect_index: 0,
            kind: "suffix".to_string(),
            mod_name: None,
            tier: None,
            tags: Vec::new(),
            text: text.to_string(),
            value,
            value_min: None,
            value_max: None,
        }
    }

    fn sorted(mut v: Vec<(Element, f64)>) -> Vec<(Element, f64)> {
        v.sort_by_key(|(e, _)| *e);
        v
    }

    #[test]
    fn single_element_resistances() {
        assert_eq!(
            resistance_from_mod(&m("+17(16-20)% to Fire Resistance", Some(17.0))),
            vec![(Element::Fire, 17.0)]
        );
        assert_eq!(
            resistance_from_mod(&m("+19(16-20)% to Cold Resistance", Some(19.0))),
            vec![(Element::Cold, 19.0)]
        );
        assert_eq!(
            resistance_from_mod(&m("+18% to Lightning Resistance", Some(18.0))),
            vec![(Element::Lightning, 18.0)]
        );
        assert_eq!(
            resistance_from_mod(&m("+11% to Chaos Resistance", Some(11.0))),
            vec![(Element::Chaos, 11.0)]
        );
    }

    #[test]
    fn all_elemental_feeds_three_and_never_chaos() {
        let got = sorted(resistance_from_mod(&m(
            "+9(9-11)% to all Elemental Resistances",
            Some(9.0),
        )));
        assert_eq!(
            got,
            vec![
                (Element::Fire, 9.0),
                (Element::Cold, 9.0),
                (Element::Lightning, 9.0)
            ]
        );
        assert!(!got.iter().any(|(e, _)| *e == Element::Chaos));
    }

    #[test]
    fn two_element_shape_feeds_both() {
        // Not present in the player's current gear, but the game has this shape.
        // Handling it now avoids a silent undercount when one turns up.
        assert_eq!(
            sorted(resistance_from_mod(&m(
                "+15% to Fire and Lightning Resistance",
                Some(15.0)
            ))),
            vec![(Element::Fire, 15.0), (Element::Lightning, 15.0)]
        );
        assert_eq!(
            sorted(resistance_from_mod(&m(
                "+13(11-15)% to Cold and Chaos Resistance",
                Some(13.0)
            ))),
            vec![(Element::Cold, 13.0), (Element::Chaos, 13.0)]
        );
    }

    #[test]
    fn the_rolled_value_is_used_not_the_bounds() {
        let mut modifier = m("+17(16-20)% to Fire Resistance", Some(17.0));
        modifier.value_min = Some(16.0);
        modifier.value_max = Some(20.0);
        assert_eq!(
            resistance_from_mod(&modifier),
            vec![(Element::Fire, 17.0)]
        );
    }

    #[test]
    fn a_modifier_with_no_value_contributes_nothing() {
        // Never guessed at: a value that failed to parse is absent, not zero-ish.
        assert!(resistance_from_mod(&m("+17% to Fire Resistance", None)).is_empty());
    }

    #[test]
    fn unrelated_modifiers_contribute_nothing() {
        assert!(resistance_from_mod(&m("+95(85-99) to maximum Life", Some(95.0))).is_empty());
        assert!(resistance_from_mod(&m("20% increased Movement Speed", Some(20.0))).is_empty());
        assert!(resistance_from_mod(&m("+3 to Level of all Minion Skills", Some(3.0))).is_empty());
    }

    #[test]
    fn resistance_penetration_is_not_resistance() {
        // Penetration reduces an enemy's resistance; counting it as the player's
        // own would inflate the total with a number that protects nothing.
        assert!(resistance_from_mod(&m(
            "Damage Penetrates 15% Fire Resistance",
            Some(15.0)
        ))
        .is_empty());
    }

    #[test]
    fn reduced_maximum_resistance_is_not_a_bonus() {
        // "+1% to maximum Fire Resistance" raises the cap, it does not add to the
        // pool. Out of scope for this feature, and must not be counted as a bonus.
        assert!(resistance_from_mod(&m(
            "+1% to maximum Fire Resistance",
            Some(1.0)
        ))
        .is_empty());
    }

    #[test]
    fn runes_count_like_any_other_modifier() {
        let mut rune = m("+18% to Fire Resistance", Some(18.0));
        rune.kind = "rune".to_string();
        assert_eq!(resistance_from_mod(&rune), vec![(Element::Fire, 18.0)]);
    }

    #[test]
    fn elemental_is_the_three_the_penalty_touches() {
        assert_eq!(
            Element::elemental(),
            [Element::Fire, Element::Cold, Element::Lightning]
        );
        assert_eq!(Element::all().len(), 4);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cd src-tauri/poe2-core && cargo test gear::resistances
```

Expected: compilation error — `cannot find type Element`.

- [ ] **Step 3: Write the implementation**

Insert above the `#[cfg(test)]` block in `src-tauri/poe2-core/src/gear/resistances.rs`:

```rust
//! Pulling resistance values out of a modifier's text.
//!
//! Three shapes carry resistance, and all three must be handled: one element,
//! all elemental, and two named elements.

use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::store::StoredMod;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum Element {
    Fire,
    Cold,
    Lightning,
    Chaos,
}

impl Element {
    pub fn as_str(&self) -> &'static str {
        match self {
            Element::Fire => "fire",
            Element::Cold => "cold",
            Element::Lightning => "lightning",
            Element::Chaos => "chaos",
        }
    }

    pub fn all() -> [Element; 4] {
        [
            Element::Fire,
            Element::Cold,
            Element::Lightning,
            Element::Chaos,
        ]
    }

    /// The three the campaign penalty applies to. Chaos is not one of them.
    pub fn elemental() -> [Element; 3] {
        [Element::Fire, Element::Cold, Element::Lightning]
    }

    fn from_name(name: &str) -> Option<Element> {
        match name {
            "Fire" => Some(Element::Fire),
            "Cold" => Some(Element::Cold),
            "Lightning" => Some(Element::Lightning),
            "Chaos" => Some(Element::Chaos),
            _ => None,
        }
    }
}

// "+17(16-20)% to Fire Resistance"
static SINGLE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"to (?P<element>Fire|Cold|Lightning|Chaos) Resistance").unwrap());
// "+9(9-11)% to all Elemental Resistances"
static ALL_ELEMENTAL_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"to all Elemental Resistances").unwrap());
// "+15% to Fire and Lightning Resistance"
static TWO_ELEMENT_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"to (?P<first>Fire|Cold|Lightning|Chaos) and (?P<second>Fire|Cold|Lightning|Chaos) Resistance",
    )
    .unwrap()
});
// "+1% to maximum Fire Resistance" raises the cap rather than the pool, and
// "Damage Penetrates 15% Fire Resistance" reduces the enemy's. Neither protects
// the player by the amount it names, so neither may be summed here.
static NOT_A_BONUS_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"maximum \w+ Resistance|Penetrat").unwrap());

/// What a modifier contributes, per element.
///
/// Returns a list because one line can feed several: all-elemental feeds three,
/// the two-element shape feeds two.
pub fn resistance_from_mod(m: &StoredMod) -> Vec<(Element, f64)> {
    let Some(value) = m.value else {
        // A value that failed to parse is absent, not zero — and never guessed at.
        return Vec::new();
    };
    if NOT_A_BONUS_RE.is_match(&m.text) {
        return Vec::new();
    }

    if let Some(caps) = TWO_ELEMENT_RE.captures(&m.text) {
        let first = caps.name("first").and_then(|x| Element::from_name(x.as_str()));
        let second = caps
            .name("second")
            .and_then(|x| Element::from_name(x.as_str()));
        return first
            .into_iter()
            .chain(second)
            .map(|element| (element, value))
            .collect();
    }

    if ALL_ELEMENTAL_RE.is_match(&m.text) {
        return Element::elemental()
            .into_iter()
            .map(|element| (element, value))
            .collect();
    }

    if let Some(caps) = SINGLE_RE.captures(&m.text) {
        if let Some(element) = caps
            .name("element")
            .and_then(|x| Element::from_name(x.as_str()))
        {
            return vec![(element, value)];
        }
    }

    Vec::new()
}
```

Add `pub mod resistances;` to `src-tauri/poe2-core/src/gear/mod.rs`.

- [ ] **Step 4: Run the tests to verify they pass**

```bash
cd src-tauri/poe2-core && cargo test
```

Expected: PASS — 10 new tests on top of the previous total.

- [ ] **Step 5: Format and commit**

```bash
cd src-tauri/poe2-core && cargo fmt
cd ../.. && git add src-tauri/poe2-core
git commit -m "feat(poe2): read resistance values from modifier text"
```

---

## Task 3: Totals, the penalty and the gaps

**Files:**
- Create: `src-tauri/poe2-core/src/gear/summary.rs`
- Modify: `src-tauri/poe2-core/src/gear/mod.rs`

**Interfaces:**
- Consumes: `Slot`, `Worn`, `infer_worn` from Task 1; `Element`, `resistance_from_mod` from Task 2; `StoredItem`.
- Produces:
  - `gear::summary::CAP: f64` = 75.0
  - `gear::summary::ResistanceLine { pub element: Element, pub from_gear: f64, pub total: Option<f64>, pub cap: f64, pub short_by: Option<f64>, pub missing_from: Vec<Slot>, pub empty_slots: Vec<Slot> }`, deriving `Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type`
  - `gear::summary::EquipmentSummary { pub lines: Vec<ResistanceLine>, pub penalty: Option<f64>, pub worn: Vec<(i64, Slot)>, pub superseded: Vec<i64>, pub unrecognised: Vec<i64>, pub empty_slots: Vec<Slot> }`, same derives
  - `gear::summary::summarise(items: &[StoredItem], excluded: &[i64], penalty: Option<f64>) -> EquipmentSummary`

`total` and `short_by` are `None` for the three elemental lines while the penalty is unset. Chaos takes no penalty, so its `total` and `short_by` are always present.

- [ ] **Step 1: Write the failing tests**

Create `src-tauri/poe2-core/src/gear/summary.rs` with only this test module:

```rust
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
        assert_eq!(
            summarise(&items, &[1], None).lines[0].from_gear,
            0.0
        );
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
        assert_eq!(fire.total, Some(0.0), "a negative resistance is not a thing here");
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
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cd src-tauri/poe2-core && cargo test gear::summary
```

Expected: compilation error — `cannot find function summarise`.

- [ ] **Step 3: Write the implementation**

Insert above the `#[cfg(test)]` block in `src-tauri/poe2-core/src/gear/summary.rs`:

```rust
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
    pub worn: Vec<(i64, Slot)>,
    pub superseded: Vec<i64>,
    pub unrecognised: Vec<i64>,
    pub empty_slots: Vec<Slot>,
}

pub fn summarise(
    items: &[StoredItem],
    excluded: &[i64],
    penalty: Option<f64>,
) -> EquipmentSummary {
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
                contributing.entry(element).or_default().insert(worn_item.slot);
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
```

Add `pub mod summary;` to `src-tauri/poe2-core/src/gear/mod.rs`.

- [ ] **Step 4: Run the tests to verify they pass**

```bash
cd src-tauri/poe2-core && cargo test
```

Expected: PASS — 14 new tests on top of the previous total.

- [ ] **Step 5: Format and commit**

```bash
cd src-tauri/poe2-core && cargo fmt
cd ../.. && git add src-tauri/poe2-core
git commit -m "feat(poe2): sum resistances, apply the penalty, name the gaps"
```

---

## Task 4: The setting, exclusions and the command

**Files:**
- Create: nothing
- Modify: `src-tauri/src/poe2/commands.rs`, `src-tauri/src/settings.rs`, `src-tauri/src/lib.rs`, `src-tauri/poe2-core/src/store.rs`

**Interfaces:**
- Consumes: `summarise`, `EquipmentSummary` from Task 3.
- Produces:
  - setting `poe2_resistance_penalty: Option<f64>`, defaulting to `None`
  - setting `poe2_excluded_items: Vec<i64>`, defaulting to empty
  - `change_poe2_resistance_penalty_setting(app: AppHandle, penalty: Option<f64>) -> Result<(), String>`
  - `poe2_set_item_excluded(app: AppHandle, item_id: i64, excluded: bool) -> Result<(), String>`
  - `poe2_equipment(app: AppHandle) -> Result<EquipmentView, String>` where
    `EquipmentView { summary: EquipmentSummary, items: Vec<EquipmentItem> }` and
    `EquipmentItem { id: i64, name: Option<String>, base_type: Option<String>, item_class: Option<String>, slot: Option<Slot>, excluded: bool, status: String }` with `status` one of `"worn"`, `"superseded"`, `"unrecognised"`, `"excluded"`
  - `Poe2Store::all_items(&self) -> anyhow::Result<Vec<StoredItem>>` — every item, not the newest fifty

The existing `items(limit)` caps at fifty by design for the list view. The calculator must see everything, or a player with more than fifty captures would silently lose gear from the totals.

- [ ] **Step 1: Add `all_items` to the store**

In `src-tauri/poe2-core/src/store.rs`, beside `items`:

```rust
    /// Every stored item, oldest first.
    ///
    /// `items(limit)` exists for the list view and caps at fifty; the calculator
    /// must see everything, or a player with more captures than that would lose
    /// gear from the totals with no sign of it.
    pub fn all_items(&self) -> Result<Vec<StoredItem>> {
        let mut stmt = self.conn.prepare("SELECT id FROM items ORDER BY id")?;
        let ids = stmt
            .query_map([], |row| row.get::<_, i64>(0))?
            .collect::<rusqlite::Result<Vec<i64>>>()?;

        let mut items = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(item) = self.item(id)? {
                items.push(item);
            }
        }
        Ok(items)
    }
```

Add a test to that file's existing `mod tests`:

```rust
    #[test]
    fn all_items_is_not_capped_like_the_list_view() {
        let mut s = store();
        let parsed = parse_item(SCEPTRE).unwrap();
        s.add_item(&parsed, "paste", Utc::now()).unwrap();
        // A second, different item so the ids differ.
        let other = parse_item(&SCEPTRE.replace("Wrath Call", "Second Item")).unwrap();
        s.add_item(&other, "paste", Utc::now()).unwrap();

        assert_eq!(s.all_items().unwrap().len(), 2);
        assert_eq!(s.items(1).unwrap().len(), 1, "the list view still caps");
    }
```

- [ ] **Step 2: Run that test to verify it fails, then passes**

```bash
cd src-tauri/poe2-core && cargo test all_items
```

Expected first: `no method named all_items`. After writing the method: PASS.

- [ ] **Step 3: Add the settings**

In `src-tauri/src/settings.rs`, inside `AppSettings`:

```rust
    /// The campaign's resistance penalty, as the player reads it off their own
    /// character panel. Unset means the calculator shows the gear contribution
    /// and withholds any comparison to the cap — a wrong number in the
    /// reassuring direction is worse than none.
    #[serde(default)]
    pub poe2_resistance_penalty: Option<f64>,
    /// Items the player has excluded from the calculation.
    #[serde(default)]
    pub poe2_excluded_items: Vec<i64>,
```

And in the `get_default_settings()` struct literal:

```rust
        poe2_resistance_penalty: None,
        poe2_excluded_items: Vec::new(),
```

- [ ] **Step 4: Add the commands**

In `src-tauri/src/poe2/commands.rs`, add the imports:

```rust
use poe2_core::gear::slots::Slot;
use poe2_core::gear::summary::{summarise, EquipmentSummary};
```

and the commands:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct EquipmentItem {
    pub id: i64,
    pub name: Option<String>,
    pub base_type: Option<String>,
    pub item_class: Option<String>,
    pub slot: Option<Slot>,
    pub excluded: bool,
    /// "worn" | "superseded" | "unrecognised" | "excluded"
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct EquipmentView {
    pub summary: EquipmentSummary,
    pub items: Vec<EquipmentItem>,
}

#[tauri::command]
#[specta::specta]
pub fn poe2_equipment(app: AppHandle) -> Result<EquipmentView, String> {
    let settings = settings::get_settings(&app);
    let excluded = settings.poe2_excluded_items.clone();

    let store = store_for(&app)?;
    let items = store.all_items().map_err(|e| e.to_string())?;
    let summary = summarise(&items, &excluded, settings.poe2_resistance_penalty);

    let worn: std::collections::BTreeMap<i64, Slot> = summary.worn.iter().copied().collect();
    let view_items = items
        .iter()
        .map(|item| {
            let is_excluded = excluded.contains(&item.id);
            let slot = worn.get(&item.id).copied();
            let status = if is_excluded {
                "excluded"
            } else if slot.is_some() {
                "worn"
            } else if summary.unrecognised.contains(&item.id) {
                "unrecognised"
            } else {
                "superseded"
            };
            EquipmentItem {
                id: item.id,
                name: item.name.clone(),
                base_type: item.base_type.clone(),
                item_class: item.item_class.clone(),
                slot,
                excluded: is_excluded,
                status: status.to_string(),
            }
        })
        .collect();

    Ok(EquipmentView {
        summary,
        items: view_items,
    })
}

#[tauri::command]
#[specta::specta]
pub fn poe2_set_item_excluded(
    app: AppHandle,
    item_id: i64,
    excluded: bool,
) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.poe2_excluded_items.retain(|id| *id != item_id);
    if excluded {
        settings.poe2_excluded_items.push(item_id);
    }
    settings::write_settings(&app, settings.clone());
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_poe2_resistance_penalty_setting(
    app: AppHandle,
    penalty: Option<f64>,
) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.poe2_resistance_penalty = penalty;
    settings::write_settings(&app, settings.clone());
    Ok(())
}
```

- [ ] **Step 5: Register the commands**

In `src-tauri/src/lib.rs`, inside `collect_commands![…]`:

```rust
            poe2::commands::poe2_equipment,
            poe2::commands::poe2_set_item_excluded,
            poe2::commands::change_poe2_resistance_penalty_setting,
```

- [ ] **Step 6: Verify**

```bash
cd src-tauri/poe2-core && cargo test
cd .. && cargo build
```

Expected: the core suite passes with one new test; the `handy` crate compiles.

- [ ] **Step 7: Format and commit**

```bash
cd src-tauri && cargo fmt
cd .. && git status --short
```

Revert any file the formatter touched that you did not edit, then:

```bash
git add src-tauri
git commit -m "feat(poe2): equipment command, penalty setting and per-item exclusion"
```

---

## Task 5: The Equipment tab

**Files:**
- Create: `src/components/poe2/EquipmentTab.tsx`
- Modify: `src/components/poe2/Poe2Page.tsx`, `src/stores/settingsStore.ts`, `src/i18n/locales/en/translation.json`, `src/i18n/locales/ru/translation.json`

**Interfaces:**
- Consumes: `commands.poe2Equipment`, `commands.poe2SetItemExcluded`, `commands.changePoe2ResistancePenaltySetting` from the generated `src/bindings.ts`.
- Produces: no interfaces for other tasks.

- [ ] **Step 1: Add the i18n keys**

In `src/i18n/locales/en/translation.json`, add `"equipment"` to the existing `"poe2.tabs"` object and a new `"poe2.equipment"` block:

```json
    "tabs": {
      "progress": "Progress",
      "items": "Items",
      "equipment": "Equipment"
    },
    "equipment": {
      "title": "Resistances",
      "fromGear": "From gear",
      "total": "Total",
      "cap": "Cap",
      "short": "short {{amount}}",
      "atCap": "at the cap",
      "noItems": "No items captured yet — copy one in the game and paste it on the Items tab.",
      "penaltyTitle": "Campaign resistance penalty",
      "penaltyUnset": "Not set. Totals below are what your gear gives, before the campaign penalty — your character panel will show less.",
      "penaltyHow": "Open the character panel in the game, compare one elemental resistance with the figure above, and enter the difference.",
      "penaltyLabel": "Penalty",
      "penaltySave": "Save",
      "penaltyClear": "Clear",
      "gapsTitle": "Where the missing points could come from",
      "emptySlots": "Nothing captured for: {{slots}}",
      "givesNothing": "Worn but giving nothing to this: {{slots}}",
      "itemsTitle": "Items counted",
      "statusWorn": "worn",
      "statusSuperseded": "replaced by something newer",
      "statusUnrecognised": "class not recognised",
      "statusExcluded": "excluded by you",
      "exclude": "Exclude",
      "include": "Include",
      "slot": {
        "weapon": "Weapon",
        "off_hand": "Off-hand",
        "body_armour": "Body armour",
        "helmet": "Helmet",
        "gloves": "Gloves",
        "boots": "Boots",
        "belt": "Belt",
        "amulet": "Amulet",
        "ring": "Ring"
      },
      "element": {
        "fire": "Fire",
        "cold": "Cold",
        "lightning": "Lightning",
        "chaos": "Chaos"
      }
    }
```

The same keys in `src/i18n/locales/ru/translation.json`:

```json
    "tabs": {
      "progress": "Прогресс",
      "items": "Предметы",
      "equipment": "Экипировка"
    },
    "equipment": {
      "title": "Сопротивления",
      "fromGear": "С вещей",
      "total": "Итог",
      "cap": "Кап",
      "short": "не хватает {{amount}}",
      "atCap": "в капе",
      "noItems": "Предметов пока нет — скопируй предмет в игре и вставь его на вкладке «Предметы».",
      "penaltyTitle": "Штраф кампании к сопротивлениям",
      "penaltyUnset": "Не задан. Ниже — то, что дают вещи, до штрафа кампании: панель персонажа покажет меньше.",
      "penaltyHow": "Открой панель персонажа в игре, сравни любое стихийное сопротивление с числом выше и введи разницу.",
      "penaltyLabel": "Штраф",
      "penaltySave": "Сохранить",
      "penaltyClear": "Сбросить",
      "gapsTitle": "Откуда взять недостающее",
      "emptySlots": "Ничего не скопировано: {{slots}}",
      "givesNothing": "Надето, но не даёт этого: {{slots}}",
      "itemsTitle": "Что посчитано",
      "statusWorn": "надето",
      "statusSuperseded": "заменено более новым",
      "statusUnrecognised": "класс не распознан",
      "statusExcluded": "исключено вами",
      "exclude": "Исключить",
      "include": "Вернуть",
      "slot": {
        "weapon": "Оружие",
        "off_hand": "Вторая рука",
        "body_armour": "Нагрудник",
        "helmet": "Шлем",
        "gloves": "Перчатки",
        "boots": "Сапоги",
        "belt": "Пояс",
        "amulet": "Амулет",
        "ring": "Кольцо"
      },
      "element": {
        "fire": "Огонь",
        "cold": "Холод",
        "lightning": "Молния",
        "chaos": "Хаос"
      }
    }
```

- [ ] **Step 2: Write the tab**

Create `src/components/poe2/EquipmentTab.tsx`:

```tsx
import React, { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands, type EquipmentView } from "../../bindings";
import { Button } from "../ui/Button";

export const EquipmentTab: React.FC = () => {
  const { t } = useTranslation();
  const [view, setView] = useState<EquipmentView | null>(null);
  const [penaltyDraft, setPenaltyDraft] = useState("");

  const load = useCallback(async () => {
    const result = await commands.poe2Equipment();
    if (result.status === "ok") {
      setView(result.data);
      setPenaltyDraft(
        result.data.summary.penalty === null
          ? ""
          : String(result.data.summary.penalty),
      );
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  const savePenalty = useCallback(async () => {
    const parsed = Number(penaltyDraft.replace(",", "."));
    if (!Number.isFinite(parsed)) return;
    await commands.changePoe2ResistancePenaltySetting(parsed);
    await load();
  }, [penaltyDraft, load]);

  const clearPenalty = useCallback(async () => {
    await commands.changePoe2ResistancePenaltySetting(null);
    await load();
  }, [load]);

  const toggleExcluded = useCallback(
    async (id: number, excluded: boolean) => {
      await commands.poe2SetItemExcluded(id, excluded);
      await load();
    },
    [load],
  );

  if (!view) return null;

  if (view.items.length === 0) {
    return <p className="p-4 text-sm opacity-70">{t("poe2.equipment.noItems")}</p>;
  }

  const slotName = (slot: string) => t(`poe2.equipment.slot.${slot}`);

  return (
    <div className="p-4 space-y-4">
      <section>
        <h2 className="text-lg font-semibold">{t("poe2.equipment.title")}</h2>
        {view.summary.penalty === null && (
          <p className="mt-1 text-sm opacity-70">{t("poe2.equipment.penaltyUnset")}</p>
        )}
        <ul className="mt-2 list-none p-0">
          {view.summary.lines.map((l) => (
            <li key={l.element} className="border-t border-mid-gray/30 py-2">
              <div className="flex flex-wrap items-baseline gap-x-3">
                <span className="w-24 font-medium">
                  {t(`poe2.equipment.element.${l.element}`)}
                </span>
                <span className="opacity-70">
                  {t("poe2.equipment.fromGear")}: {l.from_gear}%
                </span>
                {l.total !== null && (
                  <span className="font-semibold">
                    {t("poe2.equipment.total")}: {l.total}%
                  </span>
                )}
                <span className="opacity-60">
                  {t("poe2.equipment.cap")} {l.cap}%
                </span>
                {l.total !== null && (
                  <span className={l.short_by === null ? "opacity-70" : "font-semibold"}>
                    {l.short_by === null
                      ? t("poe2.equipment.atCap")
                      : t("poe2.equipment.short", { amount: l.short_by })}
                  </span>
                )}
              </div>
              {(l.missing_from.length > 0 || l.empty_slots.length > 0) && (
                <div className="mt-1 text-sm opacity-70">
                  {l.empty_slots.length > 0 && (
                    <p>
                      {t("poe2.equipment.emptySlots", {
                        slots: l.empty_slots.map(slotName).join(", "),
                      })}
                    </p>
                  )}
                  {l.missing_from.length > 0 && (
                    <p>
                      {t("poe2.equipment.givesNothing", {
                        slots: l.missing_from.map(slotName).join(", "),
                      })}
                    </p>
                  )}
                </div>
              )}
            </li>
          ))}
        </ul>
      </section>

      <section className="rounded-md border border-mid-gray/30 p-3">
        <p className="font-medium">{t("poe2.equipment.penaltyTitle")}</p>
        <p className="mt-1 text-sm opacity-70">{t("poe2.equipment.penaltyHow")}</p>
        <div className="mt-2 flex items-center gap-2">
          <label className="text-sm opacity-70" htmlFor="poe2-penalty">
            {t("poe2.equipment.penaltyLabel")}
          </label>
          <input
            id="poe2-penalty"
            value={penaltyDraft}
            onChange={(e) => setPenaltyDraft(e.target.value)}
            inputMode="numeric"
            className="w-20 rounded-md border border-mid-gray/40 bg-transparent px-2 py-1"
          />
          <Button onClick={savePenalty}>{t("poe2.equipment.penaltySave")}</Button>
          <Button onClick={clearPenalty}>{t("poe2.equipment.penaltyClear")}</Button>
        </div>
      </section>

      <section>
        <p className="font-medium">{t("poe2.equipment.itemsTitle")}</p>
        <ul className="mt-2 list-none p-0">
          {view.items.map((item) => (
            <li
              key={item.id}
              className="flex flex-wrap items-baseline gap-x-3 border-t border-mid-gray/30 py-2"
            >
              <span className="font-medium">{item.name ?? item.base_type ?? "—"}</span>
              <span className="text-sm opacity-60">
                {item.slot ? slotName(item.slot) : (item.item_class ?? "")}
              </span>
              <span className="text-sm opacity-60">
                {t(
                  `poe2.equipment.status${item.status.charAt(0).toUpperCase()}${item.status.slice(1)}`,
                )}
              </span>
              <button
                type="button"
                className="text-sm underline opacity-70"
                onClick={() => toggleExcluded(item.id, !item.excluded)}
              >
                {item.excluded ? t("poe2.equipment.include") : t("poe2.equipment.exclude")}
              </button>
            </li>
          ))}
        </ul>
      </section>
    </div>
  );
};
```

Item names come from the game log's neighbour, the clipboard, so every one of them is rendered as a React text child and never as markup.

- [ ] **Step 3: Add the tab**

In `src/components/poe2/Poe2Page.tsx`, import the new tab and widen the union:

```tsx
import { EquipmentTab } from "./EquipmentTab";
```

```tsx
type Tab = "progress" | "items" | "equipment";
```

Change the button row's array to `(["progress", "items", "equipment"] as Tab[])`, and replace the ternary with:

```tsx
      {tab === "progress" && <ProgressTab />}
      {tab === "items" && <ItemsPage />}
      {tab === "equipment" && <EquipmentTab />}
```

- [ ] **Step 4: Register the settings updater**

In `src/stores/settingsStore.ts`, add to the `settingUpdaters` map:

```ts
  poe2_resistance_penalty: (value) =>
    commands.changePoe2ResistancePenaltySetting(value as number | null),
```

`poe2_excluded_items` is written only through `poe2SetItemExcluded`, never through `updateSetting`, so it needs no entry — but if you find yourself calling `updateSetting("poe2_excluded_items", …)`, stop: the command exists so the list cannot be clobbered by a stale copy from the settings store.

- [ ] **Step 5: Verify**

```bash
bun run lint
bunx tsc --noEmit
```

Both clean. If `commands.poe2Equipment` or the `EquipmentView` type is missing from `src/bindings.ts`, run `bun run tauri dev` once to regenerate it, then stop it — never hand-edit that file.

- [ ] **Step 6: Commit**

```bash
git add src/components/poe2 src/stores/settingsStore.ts src/i18n src/bindings.ts
git commit -m "feat(poe2): equipment tab with resistances, penalty and gaps"
```

---

## Task 6: Acceptance against the player's real gear

**Files:**
- Create: `src-tauri/poe2-core/src/gear/acceptance.rs`
- Modify: `src-tauri/poe2-core/src/gear/mod.rs`

**Interfaces:**
- Consumes: everything from Tasks 1–3.
- Produces: nothing new.

These tests pin the numbers this feature was designed against, using the player's real nine items as fixtures.

- [ ] **Step 1: Write the acceptance tests**

Create `src-tauri/poe2-core/src/gear/acceptance.rs`:

```rust
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
            summary.worn.iter().filter(|(_, s)| *s == Slot::Ring).count(),
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
```

Add to `src-tauri/poe2-core/src/gear/mod.rs`:

```rust
#[cfg(test)]
mod acceptance;
```

- [ ] **Step 2: Run the full core suite**

```bash
cd src-tauri/poe2-core && cargo test
```

Expected: PASS, all of it.

- [ ] **Step 3: Verify the whole project**

```bash
cd src-tauri && cargo build
cd src-tauri/poe2-core && cargo fmt -- --check
cd .. && bun run lint
bunx tsc --noEmit
```

All clean. `cd src-tauri && cargo fmt -- --check` fails on this fork's pre-existing drift in `actions.rs` and `tray.rs` — report that as pre-existing rather than fixing unrelated files.

- [ ] **Step 4: Check it against the live database**

```bash
cd src-tauri/poe2-core && cargo test --test real_log -- --ignored --nocapture
```

That test belongs to the log tracker and must still pass unchanged — this feature touches none of its code, and a failure here means something unrelated broke.

Then, if the machine's screen is available, run the app, open the Equipment tab, and confirm the four lines read 95 / 95 / 82 / 29 before a penalty is entered. If the screen is locked or the app cannot be driven, say so plainly rather than reporting this step as done.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/poe2-core
git commit -m "test(poe2): acceptance tests over the player's real gear"
```

---

## Checking the plan against the spec's definition of done

| Criterion from the spec | Where it is met |
|---|---|
| 1. The Equipment tab lists four elements with the gear total | Task 3, `every_element_gets_a_line_even_with_no_items`; Task 5 |
| 2. With the penalty set, each line shows the final value and the distance to the cap; chaos without a penalty | Task 3, `the_penalty_applies_to_the_three_elements`, `the_penalty_never_applies_to_chaos`; Task 6, `the_penalty_reproduces_the_character_panel` |
| 3. With the penalty unset, no cap comparison, and the tab explains how to get it | Task 3, `without_a_penalty_no_cap_comparison_is_offered`; Task 6, `without_the_penalty_no_elemental_verdict_is_given`; Task 5, `penaltyUnset` and `penaltyHow` |
| 4. Runes and implicits counted; the real nine total 95 / 95 / 82 / 29 | Task 2, `runes_count_like_any_other_modifier`; Task 6, `the_real_gear_totals_what_it_should` |
| 5. Most recent per slot, two rings, superseded and unrecognised reported | Task 1, `the_most_recent_wins_a_contested_slot`, `rings_keep_the_two_most_recent`, `unrecognised_classes_are_reported_not_dropped`; Task 6, `slot_inference_over_the_real_gear` |
| 6. For each shortfall, empty slots and silent worn items are named | Task 3, `gaps_name_worn_items_giving_nothing_and_slots_standing_empty`; Task 6, `the_gaps_name_the_missing_belt_and_the_silent_slots` |
| 7. Excluding removes it from every total, and survives a restart | Task 3, `excluded_items_do_not_count`; Task 6, `excluding_the_boots_removes_their_contribution`; the restart half is the settings store, Task 4 |
| 8. Labels from i18n in English and Russian | Task 5 step 1 |
| The two-element shape is handled although unverified | Task 2, `two_element_shape_feeds_both` |
| Penetration and maximum-resistance modifiers are not summed | Task 2, `resistance_penetration_is_not_resistance`, `reduced_maximum_resistance_is_not_a_bonus` |
| The calculator sees every item, not the newest fifty | Task 4, `all_items_is_not_capped_like_the_list_view` |
| Resistances only — no life, armour or damage anywhere | Task 2, `unrelated_modifiers_contribute_nothing` |
| An item captured without advanced descriptions is flagged | **Not implemented here, deliberately.** The Items tab already shows that note per item (`poe2.items.simpleFormat`), and the totals are unaffected: a simple-format modifier still carries its rolled value, only its tier and roll range are missing. Repeating the warning on this tab would add noise without adding information. If it turns out to matter, the field is already on `StoredItem.advanced` |
