//! Parsing `Client.txt` lines. Every log regex in the project lives here.

use chrono::NaiveDateTime;
use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::json;

use crate::log::events::{Event, EventKind};

// 2026/08/02 12:05:17 9674984 3ef231e0 [DEBUG Client 23396] <message>
static LINE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"^(?P<ts>\d{4}/\d{2}/\d{2} \d{2}:\d{2}:\d{2}) \d+ [0-9a-f]+ \[(?P<level>[A-Z]+) Client \d+\] (?P<msg>.*)$",
    )
    .unwrap()
});

static AREA_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"^Generating level (?P<level>\d+) area "(?P<code>[^"]+)""#).unwrap());
static LEVEL_UP_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^: (?P<character>\S+) \((?P<ascendancy>[^)]+)\) is now level (?P<level>\d+)")
        .unwrap()
});
static REWARD_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^: (?P<character>\S+) has received (?P<reward>.+?)\.?$").unwrap());
static SLAIN_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^: (?P<name>\S+) has been slain\.").unwrap());
static SCENE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\[SCENE\] Set Source \[(?P<source>.*)\]$").unwrap());
static FOCUS_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\[WINDOW\] (?P<what>Gained|Lost) focus").unwrap());
static DISCONNECT_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^Abnormal disconnect:").unwrap());

static MARKUP_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\[(?:[^\[\]|]+\|)?([^\[\]|]+)\]").unwrap());

/// Values of `Set Source` that say nothing about a zone.
const EMPTY_SCENE: [&str; 3] = ["(null)", "(unknown)", ""];

/// Strips the game's own markup: `[A|B]` -> `B`, `[A]` -> `A`.
pub fn clean_markup(text: &str) -> String {
    MARKUP_RE.replace_all(text, "$1").into_owned()
}

/// A log line -> an event. Unrecognised lines yield `None`.
///
/// Note that the reward and slain patterns both anchor on a message starting
/// with `": "`. That is not decoration: chat messages in this log look like
/// `#channel: Someone has received ...`, and without the anchor another
/// player's chatter would be recorded as this player's quest rewards.
pub fn parse_line(line: &str) -> Option<Event> {
    let caps = LINE_RE.captures(line)?;
    let ts = NaiveDateTime::parse_from_str(caps.name("ts")?.as_str(), "%Y/%m/%d %H:%M:%S").ok()?;
    let msg = caps.name("msg")?.as_str();

    if let Some(m) = AREA_RE.captures(msg) {
        return Some(Event {
            ts,
            kind: EventKind::AreaEntered,
            payload: json!({
                "code": m.name("code")?.as_str(),
                "area_level": m.name("level")?.as_str().parse::<i64>().ok()?,
            }),
        });
    }

    if let Some(m) = LEVEL_UP_RE.captures(msg) {
        return Some(Event {
            ts,
            kind: EventKind::LevelUp,
            payload: json!({
                "character": m.name("character")?.as_str(),
                "ascendancy": m.name("ascendancy")?.as_str(),
                "level": m.name("level")?.as_str().parse::<i64>().ok()?,
            }),
        });
    }

    // Checked before the reward pattern: "X has been slain." would otherwise
    // never match, since "has received" is not the only thing that follows a name.
    if let Some(m) = SLAIN_RE.captures(msg) {
        return Some(Event {
            ts,
            kind: EventKind::Slain,
            payload: json!({ "name": m.name("name")?.as_str() }),
        });
    }

    if let Some(m) = REWARD_RE.captures(msg) {
        return Some(Event {
            ts,
            kind: EventKind::QuestReward,
            payload: json!({
                "character": m.name("character")?.as_str(),
                "reward": clean_markup(m.name("reward")?.as_str()),
            }),
        });
    }

    if let Some(m) = SCENE_RE.captures(msg) {
        let source = m.name("source")?.as_str();
        if EMPTY_SCENE.contains(&source) {
            return None;
        }
        return Some(Event {
            ts,
            kind: EventKind::Scene,
            payload: json!({ "source": source }),
        });
    }

    if let Some(m) = FOCUS_RE.captures(msg) {
        return Some(Event {
            ts,
            kind: EventKind::Focus,
            payload: json!({ "gained": m.name("what")?.as_str() == "Gained" }),
        });
    }

    if DISCONNECT_RE.is_match(msg) {
        return Some(Event {
            ts,
            kind: EventKind::Disconnect,
            payload: json!({}),
        });
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::events::EventKind;

    const SAMPLE: &str = include_str!("fixtures/sample_client.txt");

    fn parsed() -> Vec<Event> {
        SAMPLE.lines().filter_map(parse_line).collect()
    }

    #[test]
    fn level_up_is_parsed() {
        let e = parse_line(
            "2026/08/02 12:05:17 9674984 3ef231e0 [INFO Client 42864] \
             : Kasablankee (Disciple of Varashta) is now level 61",
        )
        .unwrap();
        assert_eq!(e.kind, EventKind::LevelUp);
        assert_eq!(e.payload["character"], "Kasablankee");
        assert_eq!(e.payload["ascendancy"], "Disciple of Varashta");
        assert_eq!(e.payload["level"], 61);
        assert_eq!(e.ts.to_string(), "2026-08-02 12:05:17");
    }

    #[test]
    fn area_entered_is_parsed() {
        let e = parse_line(
            "2026/08/02 17:43:48 29985703 2caa2332 [DEBUG Client 23396] \
             Generating level 59 area \"P2_2\" with seed 2251200547",
        )
        .unwrap();
        assert_eq!(e.kind, EventKind::AreaEntered);
        assert_eq!(e.payload["code"], "P2_2");
        assert_eq!(e.payload["area_level"], 59);
    }

    #[test]
    fn quest_reward_strips_markup_and_trailing_dot() {
        let e = parse_line(
            "2026/07/12 17:33:58 549544640 3ef231e0 [INFO Client 34520] \
             : Kasablankee has received +10% to [Resistances|Cold Resistance].",
        )
        .unwrap();
        assert_eq!(e.kind, EventKind::QuestReward);
        assert_eq!(e.payload["character"], "Kasablankee");
        assert_eq!(e.payload["reward"], "+10% to Cold Resistance");
    }

    #[test]
    fn single_part_markup_is_unwrapped() {
        assert_eq!(clean_markup("+1 [Charm] Slot"), "+1 Charm Slot");
    }

    #[test]
    fn chat_message_is_not_a_quest_reward() {
        // A player wrote this in the game's chat. It reads exactly like a reward
        // line, and the only thing separating them is that a real one's message
        // starts with ": " while this starts with a channel name.
        let e = parse_line(
            "2026/08/03 22:00:41 131790703 3ef231e0 [INFO Client 706388] \
             #GorillaGripMyBussy: Kasablankee has received +30 to [Spirit|Spirit].",
        );
        assert!(e.is_none(), "chat must never become a quest_reward event");
    }

    #[test]
    fn scene_is_parsed_but_null_source_is_dropped() {
        let named = parse_line(
            "2026/08/03 21:50:15 131165015 2caa2332 [INFO Client 706388] \
             [SCENE] Set Source [Clearfell Encampment]",
        )
        .unwrap();
        assert_eq!(named.kind, EventKind::Scene);
        assert_eq!(named.payload["source"], "Clearfell Encampment");

        let empty = parse_line(
            "2026/08/03 22:06:31 132140562 7fbd1225 [INFO Client 706388] \
             [SCENE] Set Source [(null)]",
        );
        assert!(
            empty.is_none(),
            "(null) carries no information about a zone"
        );
    }

    #[test]
    fn focus_both_ways() {
        let gained = parse_line(
            "2026/08/03 22:08:04 132233359 5288531e [INFO Client 706388] [WINDOW] Gained focus",
        )
        .unwrap();
        assert_eq!(gained.kind, EventKind::Focus);
        assert_eq!(gained.payload["gained"], true);

        let lost = parse_line(
            "2026/08/03 21:00:00 131000000 5288531e [INFO Client 706388] [WINDOW] Lost focus",
        )
        .unwrap();
        assert_eq!(lost.payload["gained"], false);
    }

    #[test]
    fn disconnect_and_slain() {
        let d = parse_line(
            "2026/08/03 22:06:31 132140546 2d8e8dd7 [INFO Client 706388] \
             Abnormal disconnect: An unexpected disconnection occurred.",
        )
        .unwrap();
        assert_eq!(d.kind, EventKind::Disconnect);

        let s = parse_line(
            "2026/07/12 13:51:02 536168359 3ef231e0 [INFO Client 55192] \
             : MaruniaKazaMorta has been slain.",
        )
        .unwrap();
        assert_eq!(s.kind, EventKind::Slain);
        assert_eq!(s.payload["name"], "MaruniaKazaMorta");
    }

    #[test]
    fn unrecognised_lines_yield_nothing() {
        assert!(parse_line(
            "2026/08/03 22:00:38 131787843 3ef231e0 [INFO Client 706388] \
             [SOUND] Device List changed..."
        )
        .is_none());
        assert!(parse_line("not a log line at all").is_none());
        assert!(parse_line("").is_none());
    }

    #[test]
    fn the_fixture_yields_exactly_the_expected_events() {
        let kinds: Vec<EventKind> = parsed().iter().map(|e| e.kind).collect();
        assert_eq!(
            kinds,
            vec![
                EventKind::LevelUp,
                EventKind::AreaEntered,
                EventKind::QuestReward,
                EventKind::QuestReward,
                EventKind::Scene,
                EventKind::Focus,
                EventKind::Focus,
                EventKind::Disconnect,
                EventKind::Slain,
            ]
        );
    }
}
