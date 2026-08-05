//! Pulling resistance values out of a modifier's text.
//!
//! Three shapes carry resistance, and all three must be handled: one element,
//! all elemental, and two named elements.

use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::store::StoredMod;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Type,
)]
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
        let first = caps
            .name("first")
            .and_then(|x| Element::from_name(x.as_str()));
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
        assert_eq!(resistance_from_mod(&modifier), vec![(Element::Fire, 17.0)]);
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
        assert!(
            resistance_from_mod(&m("Damage Penetrates 15% Fire Resistance", Some(15.0))).is_empty()
        );
    }

    #[test]
    fn reduced_maximum_resistance_is_not_a_bonus() {
        // "+1% to maximum Fire Resistance" raises the cap, it does not add to the
        // pool. Out of scope for this feature, and must not be counted as a bonus.
        assert!(resistance_from_mod(&m("+1% to maximum Fire Resistance", Some(1.0))).is_empty());
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
