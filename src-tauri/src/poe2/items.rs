//! Parsing item text copied out of Path of Exile 2.
//!
//! The game log says nothing about items: the only source is hovering an item
//! and pressing Ctrl+C. Every item regex in the project lives here and only
//! here.
//!
//! Parsing is driven by section content, never by section index: the number of
//! sections and their order vary from item to item. An unknown section is
//! skipped — the game adds items and affixes every league, and a parser that
//! dies on a new modifier is useless within a month.

use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::BTreeMap;
use std::fmt;

pub const MARKER: &str = "Item Class:";

static SEPARATOR_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^-{3,}$").unwrap());
static KEY_VALUE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^(?P<key>[A-Za-z][A-Za-z '/-]*):\s*(?P<value>.*)$").unwrap());
// "(augmented)", "(fractured)" and friends annotate the property, not its value.
static NOTE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)\s*\((?:augmented|unmet|fractured|enchant)\)\s*$").unwrap());
static QUALITY_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\+?(\d+)%").unwrap());
static REQUIRES_LEVEL_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)Level\s+(\d+)").unwrap());
static REQUIRES_STAT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)(\d+)\s+(Str|Dex|Int)").unwrap());

/// Keys that own a section and a meaning of their own; they never land in the
/// general property bag.
const SPECIAL_KEYS: [&str; 5] = ["Item Class", "Rarity", "Requires", "Item Level", "Sockets"];

/// The text does not look like an item copied from the game.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotAnItem;

impl fmt::Display for NotAnItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "text does not look like an item: no Item Class line")
    }
}

impl std::error::Error for NotAnItem {}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, Type)]
pub struct ParsedItem {
    pub raw_text: String,
    pub item_class: Option<String>,
    pub rarity: Option<String>,
    pub name: Option<String>,
    pub base_type: Option<String>,
    pub item_level: Option<i64>,
    pub requires_level: Option<i64>,
    /// BTreeMap rather than HashMap so serialised JSON has a stable key order:
    /// otherwise an unchanged item reparses into different bytes every run and
    /// diffing a rebuild becomes noise.
    pub requirements: BTreeMap<String, i64>,
    pub quality: Option<i64>,
    pub sockets: Option<String>,
    pub properties: BTreeMap<String, String>,
    pub advanced: bool,
}

/// A strict test: only text starting with `Item Class:` is processed.
///
/// The strictness is a safety requirement, not fussiness. When clipboard
/// watching is on, everything the player copies passes through here — including
/// passwords out of a password manager.
pub fn looks_like_item(text: &str) -> bool {
    text.trim_start().starts_with(MARKER)
}

/// Text -> sections separated by a line of dashes. Blank lines are dropped.
pub fn split_sections(text: &str) -> Vec<Vec<String>> {
    let mut sections: Vec<Vec<String>> = Vec::new();
    let mut current: Vec<String> = Vec::new();

    for raw in text.replace("\r\n", "\n").split('\n') {
        let line = raw.trim();
        if SEPARATOR_RE.is_match(line) {
            if !current.is_empty() {
                sections.push(std::mem::take(&mut current));
            }
            continue;
        }
        if !line.is_empty() {
            current.push(line.to_string());
        }
    }
    if !current.is_empty() {
        sections.push(current);
    }
    sections
}

fn key_value(line: &str) -> Option<(String, String)> {
    let caps = KEY_VALUE_RE.captures(line)?;
    let key = caps.name("key")?.as_str().trim().to_string();
    let value = NOTE_RE
        .replace(caps.name("value")?.as_str(), "")
        .trim()
        .to_string();
    Some((key, value))
}

struct Header {
    item_class: Option<String>,
    rarity: Option<String>,
    name: Option<String>,
    base_type: Option<String>,
}

fn parse_header(lines: &[String]) -> Header {
    let mut item_class = None;
    let mut rarity = None;
    let mut names: Vec<String> = Vec::new();

    for line in lines {
        // Only these two count as keys in the header: an item's name can contain
        // a colon too, and mistaking it for a key would lose the name.
        match key_value(line) {
            Some((key, value)) if key == "Item Class" => item_class = Some(value),
            Some((key, value)) if key == "Rarity" => rarity = Some(value),
            _ => names.push(line.clone()),
        }
    }

    let (name, base_type) = match names.len() {
        0 => (None, None),
        // A normal item has no name, only a base type.
        1 => (None, Some(names[0].clone())),
        _ => (Some(names[0].clone()), Some(names[1].clone())),
    };

    Header {
        item_class,
        rarity,
        name,
        base_type,
    }
}

/// "Level 51, 45 Str, 45 Int" -> (51, {"Str": 45, "Int": 45})
fn parse_requires(value: &str) -> (Option<i64>, BTreeMap<String, i64>) {
    let level = REQUIRES_LEVEL_RE
        .captures(value)
        .and_then(|c| c.get(1))
        .and_then(|m| m.as_str().parse().ok());

    let mut stats = BTreeMap::new();
    for caps in REQUIRES_STAT_RE.captures_iter(value) {
        let amount: i64 = match caps[1].parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        let stat = caps[2].to_lowercase();
        let mut chars = stat.chars();
        let capitalised = match chars.next() {
            Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
            None => continue,
        };
        stats.insert(capitalised, amount);
    }
    (level, stats)
}

/// Item text -> structure. Returns `NotAnItem` when the text is not an item.
pub fn parse_item(text: &str) -> Result<ParsedItem, NotAnItem> {
    if !looks_like_item(text) {
        return Err(NotAnItem);
    }

    let sections = split_sections(text);
    let header = sections
        .first()
        .map(|lines| parse_header(lines))
        .unwrap_or(Header {
            item_class: None,
            rarity: None,
            name: None,
            base_type: None,
        });

    let mut item = ParsedItem {
        raw_text: text.to_string(),
        item_class: header.item_class,
        rarity: header.rarity,
        name: header.name,
        base_type: header.base_type,
        ..Default::default()
    };

    for section in sections.iter().skip(1) {
        let pairs: Vec<Option<(String, String)>> =
            section.iter().map(|line| key_value(line)).collect();
        if pairs.iter().any(Option::is_none) {
            // Not key-value pairs: modifiers or something unfamiliar. Both are
            // handled separately (tasks 2 and 3).
            continue;
        }
        for (key, value) in pairs.into_iter().flatten() {
            match key.as_str() {
                "Item Level" => item.item_level = value.parse().ok(),
                "Requires" => {
                    let (level, stats) = parse_requires(&value);
                    item.requires_level = level;
                    item.requirements = stats;
                }
                "Sockets" => item.sockets = Some(value),
                other if !SPECIAL_KEYS.contains(&other) => {
                    if other == "Quality" {
                        item.quality = QUALITY_RE
                            .captures(&value)
                            .and_then(|c| c.get(1))
                            .and_then(|m| m.as_str().parse().ok());
                    }
                    item.properties.insert(key, value);
                }
                _ => {}
            }
        }
    }

    Ok(item)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCEPTRE: &str = include_str!("fixtures/sceptre.txt");
    const BODY_ARMOUR: &str = include_str!("fixtures/body_armour.txt");

    #[test]
    fn looks_like_item_accepts_only_real_items() {
        assert!(looks_like_item("Item Class: Sceptres\nRarity: Rare"));
        assert!(looks_like_item("  Item Class: Sceptres\n"));
        assert!(!looks_like_item("my password from the password manager"));
        assert!(!looks_like_item(""));
    }

    #[test]
    fn text_without_item_class_is_rejected() {
        assert!(parse_item("some entirely unrelated text").is_err());
    }

    #[test]
    fn sections_are_split_by_dashes() {
        let sections = split_sections(SCEPTRE);
        assert_eq!(sections[0][0], "Item Class: Sceptres");
        assert_eq!(
            sections[1],
            vec!["Quality: +20% (augmented)", "Spirit: 166 (augmented)"]
        );
        assert_eq!(sections.len(), 7);
    }

    #[test]
    fn windows_line_endings_do_not_break_sections() {
        let sections = split_sections("Item Class: Sceptres\r\n--------\r\nItem Level: 58\r\n");
        assert_eq!(
            sections,
            vec![vec!["Item Class: Sceptres"], vec!["Item Level: 58"]]
        );
    }

    #[test]
    fn sceptre_header() {
        let item = parse_item(SCEPTRE).unwrap();
        assert_eq!(item.item_class.as_deref(), Some("Sceptres"));
        assert_eq!(item.rarity.as_deref(), Some("Rare"));
        assert_eq!(item.name.as_deref(), Some("Wrath Call"));
        assert_eq!(item.base_type.as_deref(), Some("Rattling Sceptre"));
    }

    #[test]
    fn sceptre_numbers() {
        let item = parse_item(SCEPTRE).unwrap();
        assert_eq!(item.item_level, Some(58));
        assert_eq!(item.requires_level, Some(44));
        assert_eq!(item.quality, Some(20));
        assert_eq!(item.sockets.as_deref(), Some("S"));
    }

    #[test]
    fn properties_keep_unknown_keys() {
        let item = parse_item(SCEPTRE).unwrap();
        assert_eq!(
            item.properties.get("Spirit").map(String::as_str),
            Some("166")
        );
        assert_eq!(
            item.properties.get("Grants Skill").map(String::as_str),
            Some("Level 14 Skeletal Warrior Minion")
        );
    }

    #[test]
    fn defences_are_pairs_not_fixed_fields() {
        // Runic Ward appears in no older reference: the unknown is kept, not dropped.
        let item = parse_item(BODY_ARMOUR).unwrap();
        assert_eq!(
            item.properties.get("Armour").map(String::as_str),
            Some("392")
        );
        assert_eq!(
            item.properties.get("Energy Shield").map(String::as_str),
            Some("113")
        );
        assert_eq!(
            item.properties.get("Runic Ward").map(String::as_str),
            Some("104")
        );
    }

    #[test]
    fn requirements_are_split() {
        let item = parse_item(BODY_ARMOUR).unwrap();
        assert_eq!(item.requires_level, Some(51));
        assert_eq!(item.requirements.get("Str"), Some(&45));
        assert_eq!(item.requirements.get("Int"), Some(&45));
        assert_eq!(item.requirements.len(), 2);
    }

    #[test]
    fn sockets_of_body_armour() {
        assert_eq!(
            parse_item(BODY_ARMOUR).unwrap().sockets.as_deref(),
            Some("S S")
        );
    }

    #[test]
    fn raw_text_is_kept_verbatim() {
        assert_eq!(parse_item(SCEPTRE).unwrap().raw_text, SCEPTRE);
    }

    #[test]
    fn normal_item_has_base_but_no_name() {
        let item = parse_item("Item Class: Sceptres\nRarity: Normal\nRattling Sceptre\n").unwrap();
        assert_eq!(item.name, None);
        assert_eq!(item.base_type.as_deref(), Some("Rattling Sceptre"));
    }

    #[test]
    fn unknown_section_is_skipped_without_crashing() {
        let text = "Item Class: Sceptres\nRarity: Rare\nWrath Call\nRattling Sceptre\n\
                    --------\nSomething the parser has never seen\n--------\nItem Level: 58\n";
        assert_eq!(parse_item(text).unwrap().item_level, Some(58));
    }
}
