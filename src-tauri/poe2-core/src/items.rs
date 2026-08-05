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

// 50(45-50) — the rolled value plus the tier bounds. Negative bounds occur too:
// "-10(-12--8)", where the separator is the third dash in a row.
static RANGE_VALUE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(-?\d+(?:\.\d+)?)\((-?\d+(?:\.\d+)?)-(-?\d+(?:\.\d+)?)\)").unwrap());
static PLAIN_VALUE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"[-+]?\d+(?:\.\d+)?").unwrap());

// "+18% to Fire Resistance (rune)" — an inserted rune, not an affix of the item.
static SOURCE_SUFFIX_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)\s*\((rune|implicit|crafted|enchant)\)\s*$").unwrap());

// { Crafted Suffix Modifier "of the Stars" (Tier: 4) — Minion, Gem }
// The kind group absorbs every leading word, not just one: real headers can read
// "Fractured Prefix Modifier" or "Desecrated Suffix Modifier", where only the
// last word is the kind we care about. Matching just one word would fail the
// whole header on the extra word and lose the name and tier along with it.
static MOD_HEADER_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"^\{\s*(?P<crafted>Crafted\s+)?(?P<kind>(?:[A-Za-z]+\s+)+)?Modifier(?:\s+"(?P<name>[^"]*)")?(?:\s*\(Tier:\s*(?P<tier>\d+)\))?(?:\s*[—–-]\s*(?P<tags>[^}]*?))?\s*\}$"#,
    )
    .unwrap()
});

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModKind {
    Prefix,
    Suffix,
    Implicit,
    Crafted,
    Rune,
    Unknown,
}

impl ModKind {
    /// Stored in SQLite as text rather than an integer discriminant, so a
    /// hand-run SELECT stays readable and adding a kind later renumbers nothing.
    pub fn as_str(&self) -> &'static str {
        match self {
            ModKind::Prefix => "prefix",
            ModKind::Suffix => "suffix",
            ModKind::Implicit => "implicit",
            ModKind::Crafted => "crafted",
            ModKind::Rune => "rune",
            ModKind::Unknown => "unknown",
        }
    }

    pub fn from_str_or_unknown(raw: &str) -> ModKind {
        match raw.to_lowercase().as_str() {
            "prefix" => ModKind::Prefix,
            "suffix" => ModKind::Suffix,
            "implicit" => ModKind::Implicit,
            "crafted" => ModKind::Crafted,
            "rune" => ModKind::Rune,
            _ => ModKind::Unknown,
        }
    }
}

/// A value with its roll bounds. Bounds are known only in the advanced format.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModValue {
    pub value: f64,
    pub value_min: Option<f64>,
    pub value_max: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModEffect {
    pub text: String,
    pub values: Vec<ModValue>,
}

/// One modifier, which may span several lines of text.
///
/// Parsing works in blocks — "a header in braces plus every line until the next
/// header" — not line by line: the Counselor's prefix grants both spirit and
/// mana, and line-by-line parsing would turn it into two separate modifiers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ItemMod {
    pub kind: ModKind,
    pub name: Option<String>,
    pub tier: Option<i64>,
    pub tags: Vec<String>,
    pub effects: Vec<ModEffect>,
}

impl ItemMod {
    pub fn text(&self) -> String {
        self.effects
            .iter()
            .map(|e| e.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
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
    pub mods: Vec<ItemMod>,
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
fn split_sections(text: &str) -> Vec<Vec<String>> {
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

/// The numbers in a modifier. In the advanced format they come with roll bounds.
///
/// A ranged value and a plain value can appear in the same line, e.g. a granted
/// skill level next to a rolled percentage. The ranged spans are matched first
/// and blanked out of a working copy of the text so the plain pass never
/// re-matches their digits (which would split "22(21-24)" into 22, 21 and 24),
/// then both passes are merged back into source order.
fn parse_values(text: &str) -> Vec<ModValue> {
    let mut found: Vec<(usize, ModValue)> = Vec::new();
    let mut masked = text.as_bytes().to_vec();

    for caps in RANGE_VALUE_RE.captures_iter(text) {
        let whole = match caps.get(0) {
            Some(m) => m,
            None => continue,
        };
        let value = match caps.get(1).and_then(|m| m.as_str().parse().ok()) {
            Some(v) => v,
            None => continue,
        };
        let value_min = caps.get(2).and_then(|m| m.as_str().parse().ok());
        let value_max = caps.get(3).and_then(|m| m.as_str().parse().ok());
        found.push((
            whole.start(),
            ModValue {
                value,
                value_min,
                value_max,
            },
        ));
        // Ranged spans are pure ASCII, so overwriting their bytes with spaces
        // keeps the rest of the (possibly non-ASCII) text valid UTF-8.
        for b in &mut masked[whole.start()..whole.end()] {
            *b = b' ';
        }
    }

    let masked_text = String::from_utf8(masked).unwrap_or_default();
    for m in PLAIN_VALUE_RE.find_iter(&masked_text) {
        if let Ok(value) = m.as_str().parse() {
            found.push((
                m.start(),
                ModValue {
                    value,
                    value_min: None,
                    value_max: None,
                },
            ));
        }
    }

    found.sort_by_key(|(pos, _)| *pos);
    found.into_iter().map(|(_, value)| value).collect()
}

/// A modifier header -> (kind, name, tier, tags). Not a header -> None.
///
/// Anything the game wrapped in braces counts as a header. A line that cannot be
/// parsed in detail stays a header of unknown kind: turning it into an effect
/// would write game bookkeeping into the modifier's own description.
fn parse_mod_header(line: &str) -> Option<(ModKind, Option<String>, Option<i64>, Vec<String>)> {
    let stripped = line.trim();
    if !(stripped.starts_with('{') && stripped.ends_with('}')) {
        return None;
    }

    let caps = match MOD_HEADER_RE.captures(stripped) {
        Some(caps) => caps,
        None => return Some((ModKind::Unknown, None, None, Vec::new())),
    };

    let kind = if caps.name("crafted").is_some() {
        ModKind::Crafted
    } else {
        match caps.name("kind") {
            // "Fractured Prefix " -> only the last word carries the kind; the
            // leading words are qualifiers the format doesn't otherwise expose.
            Some(m) => {
                ModKind::from_str_or_unknown(m.as_str().split_whitespace().last().unwrap_or(""))
            }
            None => ModKind::Unknown,
        }
    };

    let name = caps.name("name").map(|m| m.as_str().to_string());
    let tier = caps
        .name("tier")
        .and_then(|m| m.as_str().parse::<i64>().ok());
    let tags = caps
        .name("tags")
        .map(|m| {
            m.as_str()
                .split(',')
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();

    Some((kind, name, tier, tags))
}

/// An advanced-format section -> modifiers.
///
/// Lines before the first header are kept with kind Unknown so nothing is lost.
fn parse_mod_block(lines: &[String]) -> Vec<ItemMod> {
    let mut mods: Vec<ItemMod> = Vec::new();
    let mut header: Option<(ModKind, Option<String>, Option<i64>, Vec<String>)> = None;
    let mut effects: Vec<ModEffect> = Vec::new();

    for line in lines {
        if let Some(parsed) = parse_mod_header(line) {
            flush_mod(&mut mods, &header, &mut effects);
            header = Some(parsed);
            continue;
        }
        effects.push(ModEffect {
            text: line.clone(),
            values: parse_values(line),
        });
    }
    flush_mod(&mut mods, &header, &mut effects);
    mods
}

fn flush_mod(
    mods: &mut Vec<ItemMod>,
    header: &Option<(ModKind, Option<String>, Option<i64>, Vec<String>)>,
    effects: &mut Vec<ModEffect>,
) {
    if effects.is_empty() {
        return;
    }
    let (kind, name, tier, tags) = match header {
        Some((kind, name, tier, tags)) => (*kind, name.clone(), *tier, tags.clone()),
        None => (ModKind::Unknown, None, None, Vec::new()),
    };
    mods.push(ItemMod {
        kind,
        name,
        tier,
        tags,
        effects: std::mem::take(effects),
    });
}

/// A section with no braces: runes and simple-format modifiers.
///
/// In the simple format (advanced descriptions off in the game) only the text is
/// known — no kind, name, tier, or range. Parsing does not fail; it simply
/// extracts less.
fn parse_plain_mods(lines: &[String]) -> Vec<ItemMod> {
    lines
        .iter()
        .map(|line| {
            let kind = SOURCE_SUFFIX_RE
                .captures(line)
                .map(|caps| ModKind::from_str_or_unknown(&caps[1]))
                .unwrap_or(ModKind::Unknown);
            let text = SOURCE_SUFFIX_RE.replace(line, "").to_string();
            let values = parse_values(&text);
            ItemMod {
                kind,
                name: None,
                tier: None,
                tags: Vec::new(),
                effects: vec![ModEffect { text, values }],
            }
        })
        .collect()
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
        if section.iter().any(|line| line.starts_with('{')) {
            // Braces appear only when the game's advanced item descriptions are
            // enabled. Their presence is the mode indicator.
            item.advanced = true;
            item.mods.extend(parse_mod_block(section));
            continue;
        }
        // Classified per line, not per section: a section can mix a real
        // `Key: value` property with a bare rune or simple-format modifier
        // line, and treating the whole section as "not key-value pairs" would
        // turn the property line itself into a bogus modifier (its number
        // would then be read as a rolled value).
        let mut plain_lines: Vec<String> = Vec::new();
        for line in section {
            let Some((key, value)) = key_value(line) else {
                plain_lines.push(line.clone());
                continue;
            };
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
        if !plain_lines.is_empty() {
            // Runes or simple-format modifiers. The unknown is kept, not
            // discarded.
            item.mods.extend(parse_plain_mods(&plain_lines));
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
    fn unknown_section_becomes_a_plain_modifier_without_crashing() {
        let text = "Item Class: Sceptres\nRarity: Rare\nWrath Call\nRattling Sceptre\n\
                    --------\nSomething the parser has never seen\n--------\nItem Level: 58\n";
        assert_eq!(parse_item(text).unwrap().item_level, Some(58));
    }

    #[test]
    fn value_with_roll_range() {
        assert_eq!(
            parse_values("50(45-50)% increased Spirit"),
            vec![ModValue {
                value: 50.0,
                value_min: Some(45.0),
                value_max: Some(50.0)
            }]
        );
    }

    #[test]
    fn value_without_range() {
        assert_eq!(
            parse_values("+22 to maximum Mana"),
            vec![ModValue {
                value: 22.0,
                value_min: None,
                value_max: None
            }]
        );
    }

    #[test]
    fn two_ranges_in_one_effect() {
        // "from and to", where each half carries its own roll range.
        assert_eq!(
            parse_values("15(11-16) to 23(17-23) Physical Thorns damage"),
            vec![
                ModValue {
                    value: 15.0,
                    value_min: Some(11.0),
                    value_max: Some(16.0)
                },
                ModValue {
                    value: 23.0,
                    value_min: Some(17.0),
                    value_max: Some(23.0)
                },
            ]
        );
    }

    #[test]
    fn negative_range_is_parsed() {
        assert_eq!(
            parse_values("-10(-12--8)% reduced something"),
            vec![ModValue {
                value: -10.0,
                value_min: Some(-12.0),
                value_max: Some(-8.0)
            }]
        );
    }

    #[test]
    fn effect_without_numbers_has_no_values() {
        assert!(parse_values("Minions cannot be Poisoned").is_empty());
    }

    #[test]
    fn plain_value_alongside_a_range_is_kept_in_source_order() {
        assert_eq!(
            parse_values("+3 to Level of all Minion Skills and 50(45-50)% increased Something"),
            vec![
                ModValue {
                    value: 3.0,
                    value_min: None,
                    value_max: None
                },
                ModValue {
                    value: 50.0,
                    value_min: Some(45.0),
                    value_max: Some(50.0)
                },
            ]
        );
    }

    #[test]
    fn mod_header_prefix() {
        assert_eq!(
            parse_mod_header("{ Prefix Modifier \"Count's\" (Tier: 4) }"),
            Some((
                ModKind::Prefix,
                Some("Count's".to_string()),
                Some(4),
                vec![]
            ))
        );
    }

    #[test]
    fn mod_header_with_tags() {
        assert_eq!(
            parse_mod_header("{ Suffix Modifier \"of the Overseer\" (Tier: 2) — Minion, Gem }"),
            Some((
                ModKind::Suffix,
                Some("of the Overseer".to_string()),
                Some(2),
                vec!["Minion".to_string(), "Gem".to_string()]
            ))
        );
    }

    #[test]
    fn two_word_kind_resolves_to_the_last_word() {
        assert_eq!(
            parse_mod_header("{ Fractured Prefix Modifier \"of the Whale\" (Tier: 3) }"),
            Some((
                ModKind::Prefix,
                Some("of the Whale".to_string()),
                Some(3),
                vec![]
            ))
        );
    }

    #[test]
    fn two_word_kind_with_tags() {
        assert_eq!(
            parse_mod_header("{ Desecrated Suffix Modifier \"of Ruin\" (Tier: 1) — Chaos }"),
            Some((
                ModKind::Suffix,
                Some("of Ruin".to_string()),
                Some(1),
                vec!["Chaos".to_string()]
            ))
        );
    }

    #[test]
    fn crafted_mod_has_its_own_kind_and_no_tier() {
        assert_eq!(
            parse_mod_header("{ Crafted Suffix Modifier \"of the Stars\" — Minion }"),
            Some((
                ModKind::Crafted,
                Some("of the Stars".to_string()),
                None,
                vec!["Minion".to_string()]
            ))
        );
    }

    #[test]
    fn implicit_mod_header() {
        assert_eq!(
            parse_mod_header("{ Implicit Modifier — Attack }"),
            Some((ModKind::Implicit, None, None, vec!["Attack".to_string()]))
        );
    }

    #[test]
    fn ordinary_line_is_not_a_mod_header() {
        assert_eq!(parse_mod_header("+22(21-24) to maximum Mana"), None);
    }

    #[test]
    fn unreadable_header_in_braces_is_still_a_header() {
        // The unknown must not break parsing, and must not masquerade as an effect.
        assert_eq!(
            parse_mod_header("{ Peculiar Modifier of an unknown shape and form }"),
            Some((ModKind::Unknown, None, None, vec![]))
        );
    }

    #[test]
    fn sceptre_has_four_mods() {
        let item = parse_item(SCEPTRE).unwrap();
        assert_eq!(item.mods.len(), 4);
        let kinds: Vec<ModKind> = item.mods.iter().map(|m| m.kind).collect();
        assert_eq!(
            kinds,
            vec![
                ModKind::Prefix,
                ModKind::Prefix,
                ModKind::Suffix,
                ModKind::Crafted
            ]
        );
    }

    #[test]
    fn multiline_mod_is_one_mod_with_two_effects() {
        // The central check: Counselor's granted both spirit and mana.
        let item = parse_item(SCEPTRE).unwrap();
        let counselor = item
            .mods
            .iter()
            .find(|m| m.name.as_deref() == Some("Counselor's"))
            .unwrap();
        assert_eq!(counselor.effects.len(), 2);
        assert_eq!(
            counselor.effects[0].values,
            vec![ModValue {
                value: 16.0,
                value_min: Some(15.0),
                value_max: Some(18.0)
            }]
        );
        assert_eq!(
            counselor.effects[1].values,
            vec![ModValue {
                value: 22.0,
                value_min: Some(21.0),
                value_max: Some(24.0)
            }]
        );
    }

    #[test]
    fn mod_carries_tier_and_tags() {
        let item = parse_item(SCEPTRE).unwrap();
        let overseer = item
            .mods
            .iter()
            .find(|m| m.name.as_deref() == Some("of the Overseer"))
            .unwrap();
        assert_eq!(overseer.tier, Some(2));
        assert_eq!(overseer.tags, vec!["Minion".to_string(), "Gem".to_string()]);
    }

    #[test]
    fn mod_text_joins_its_effects() {
        let item = parse_item(SCEPTRE).unwrap();
        let counselor = item
            .mods
            .iter()
            .find(|m| m.name.as_deref() == Some("Counselor's"))
            .unwrap();
        assert_eq!(
            counselor.text(),
            "16(15-18)% increased Spirit\n+22(21-24) to maximum Mana"
        );
    }

    #[test]
    fn advanced_format_is_detected() {
        assert!(parse_item(SCEPTRE).unwrap().advanced);
    }

    #[test]
    fn armour_mods_include_two_range_effect() {
        let item = parse_item(BODY_ARMOUR).unwrap();
        let barbed = item
            .mods
            .iter()
            .find(|m| m.name.as_deref() == Some("Barbed"))
            .unwrap();
        assert_eq!(
            barbed.effects[0].values,
            vec![
                ModValue {
                    value: 15.0,
                    value_min: Some(11.0),
                    value_max: Some(16.0)
                },
                ModValue {
                    value: 23.0,
                    value_min: Some(17.0),
                    value_max: Some(23.0)
                },
            ]
        );
    }

    const SIMPLE_FORMAT: &str = "Item Class: Sceptres\nRarity: Rare\nWrath Call\n\
                                 Rattling Sceptre\n--------\nRequires: Level 44\n\
                                 --------\nItem Level: 58\n--------\n\
                                 50% increased Spirit\n+22 to maximum Mana\n";

    #[test]
    fn runes_are_kept_as_mods() {
        let item = parse_item(BODY_ARMOUR).unwrap();
        let runes: Vec<&ItemMod> = item
            .mods
            .iter()
            .filter(|m| m.kind == ModKind::Rune)
            .collect();
        assert_eq!(runes.len(), 2);
        assert_eq!(runes[0].text(), "+5% to all Elemental Resistances");
        assert_eq!(
            runes[1].effects[0].values,
            vec![ModValue {
                value: 18.0,
                value_min: None,
                value_max: None
            }]
        );
    }

    #[test]
    fn runes_do_not_become_item_affixes() {
        // A rune is an insert, not an affix: it has neither tier nor name.
        let item = parse_item(BODY_ARMOUR).unwrap();
        let rune = item.mods.iter().find(|m| m.kind == ModKind::Rune).unwrap();
        assert_eq!(rune.tier, None);
        assert_eq!(rune.name, None);
    }

    #[test]
    fn armour_has_three_affixes_besides_runes() {
        let item = parse_item(BODY_ARMOUR).unwrap();
        let kinds: Vec<ModKind> = item
            .mods
            .iter()
            .filter(|m| m.kind != ModKind::Rune)
            .map(|m| m.kind)
            .collect();
        assert_eq!(
            kinds,
            vec![ModKind::Prefix, ModKind::Prefix, ModKind::Suffix]
        );
    }

    #[test]
    fn simple_format_parses_without_crashing() {
        let item = parse_item(SIMPLE_FORMAT).unwrap();
        assert!(!item.advanced);
        assert_eq!(item.item_level, Some(58));
        let kinds: Vec<ModKind> = item.mods.iter().map(|m| m.kind).collect();
        assert_eq!(kinds, vec![ModKind::Unknown, ModKind::Unknown]);
    }

    #[test]
    fn simple_format_still_extracts_values() {
        let item = parse_item(SIMPLE_FORMAT).unwrap();
        assert_eq!(
            item.mods[0].effects[0].values,
            vec![ModValue {
                value: 50.0,
                value_min: None,
                value_max: None
            }]
        );
        assert_eq!(item.mods[0].name, None);
    }

    #[test]
    fn unknown_modifier_is_kept_as_text() {
        let text = "Item Class: Sceptres\nRarity: Rare\nWrath Call\nRattling Sceptre\n\
                    --------\n{ Peculiar Modifier of an unknown shape }\n\
                    A brand new effect with no numbers\n";
        let item = parse_item(text).unwrap();
        assert_eq!(item.mods[0].kind, ModKind::Unknown);
        assert_eq!(
            item.mods[0].effects[0].text,
            "A brand new effect with no numbers"
        );
    }

    #[test]
    fn property_line_next_to_a_bare_line_is_not_turned_into_a_modifier() {
        // A section can mix a real `Key: value` property with a bare line
        // (e.g. a "Corrupted" flag). The property line must stay a property,
        // not get swept into parse_plain_mods along with the bare line.
        let text = "Item Class: Sceptres\nRarity: Rare\nWrath Call\nRattling Sceptre\n\
                    --------\nItem Level: 53\nCorrupted\n";
        let item = parse_item(text).unwrap();
        assert_eq!(item.item_level, Some(53));
        assert_eq!(item.mods.len(), 1);
        assert_eq!(item.mods[0].text(), "Corrupted");
        assert!(!item.mods.iter().any(|m| m.text().contains("Item Level")));
    }

    #[test]
    fn property_line_does_not_leak_its_number_into_mod_values() {
        let text = "Item Class: Sceptres\nRarity: Rare\nWrath Call\nRattling Sceptre\n\
                    --------\nItem Level: 53\nCorrupted\n";
        let item = parse_item(text).unwrap();
        let leaked = item
            .mods
            .iter()
            .flat_map(|m| &m.effects)
            .flat_map(|e| &e.values)
            .any(|v| v.value == 53.0);
        assert!(!leaked);
    }
}
