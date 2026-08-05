//! Folding a stream of events into the player's current state. Pure functions.

use chrono::NaiveDateTime;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::log::events::{Event, EventKind};

static ACT_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^Act \d+$").unwrap());

/// No `specta::Type` here: the timestamps are `NaiveDateTime`, which specta only
/// knows with its chrono feature enabled. The Tauri layer flattens what the
/// frontend needs into its own type with string timestamps, and this one keeps
/// real dates, where arithmetic on them belongs.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TrackerState {
    pub character: Option<String>,
    pub ascendancy: Option<String>,
    pub level: Option<i64>,
    pub zone_code: Option<String>,
    pub zone_level: Option<i64>,
    pub zone_since: Option<NaiveDateTime>,
    pub act: Option<String>,
    /// When the active character's identity was last confirmed by a level-up or
    /// a quest reward.
    ///
    /// The active character is whoever's name appeared last, so after switching
    /// to a character who has not yet levelled or taken a reward, the previous
    /// one is still shown. This lets the interface say how fresh that claim is
    /// instead of asserting it flatly.
    pub character_confirmed_ts: Option<NaiveDateTime>,
    pub focused: bool,
    pub last_ts: Option<NaiveDateTime>,
    /// BTreeMap rather than HashMap so serialised state has a stable key order —
    /// otherwise identical state serialises differently between runs.
    pub rewards: BTreeMap<String, Vec<String>>,
    pub known_characters: BTreeMap<String, String>,
}

/// Switching the active character clears their level: it is unknown until they
/// level up, and the previous character's level against the new character's zone
/// would produce a confident, wrong level gap.
fn switch_character(state: &mut TrackerState, name: &str) {
    if state.character.as_deref() == Some(name) {
        return;
    }
    state.character = Some(name.to_string());
    state.level = None;
    state.ascendancy = state.known_characters.get(name).cloned();
}

pub fn reduce(state: TrackerState, event: &Event) -> TrackerState {
    let mut next = state;
    next.last_ts = Some(event.ts);

    match event.kind {
        EventKind::LevelUp => {
            // An event with no (string) character name carries no information
            // about who is playing. Treating it as a switch to a character
            // named "" would displace whoever is actually active.
            if let Some(name) = event.payload["character"].as_str() {
                let name = name.to_string();
                let ascendancy = event.payload["ascendancy"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                switch_character(&mut next, &name);
                next.character = Some(name.clone());
                // Authoritative for this arm: the level-up payload always
                // carries the character's current ascendancy, so it wins over
                // whatever switch_character looked up in known_characters.
                // That lookup only matters on the QuestReward path below,
                // where no ascendancy is carried in the payload.
                next.ascendancy = Some(ascendancy.clone());
                // A malformed level must not wipe a level that was already
                // known, whether or not the character changed.
                if let Some(level) = event.payload["level"].as_i64() {
                    next.level = Some(level);
                }
                next.character_confirmed_ts = Some(event.ts);
                next.known_characters.insert(name, ascendancy);
            }
        }
        EventKind::QuestReward => {
            if let Some(name) = event.payload["character"].as_str() {
                let name = name.to_string();
                switch_character(&mut next, &name);
                if let Some(reward) = event.payload["reward"].as_str() {
                    next.rewards
                        .entry(name.clone())
                        .or_default()
                        .push(reward.to_string());
                }
                next.character = Some(name);
                next.character_confirmed_ts = Some(event.ts);
            }
        }
        EventKind::AreaEntered => {
            next.zone_code = event.payload["code"].as_str().map(str::to_string);
            next.zone_level = event.payload["area_level"].as_i64();
            next.zone_since = Some(event.ts);
        }
        EventKind::Scene => {
            if let Some(source) = event.payload["source"].as_str() {
                if ACT_RE.is_match(source) {
                    next.act = Some(source.to_string());
                }
            }
        }
        EventKind::Focus => {
            next.focused = event.payload["gained"].as_bool().unwrap_or(false);
        }
        EventKind::Disconnect | EventKind::Slain => {}
    }

    next
}

pub fn build_state<'a>(events: impl IntoIterator<Item = &'a Event>) -> TrackerState {
    events.into_iter().fold(TrackerState::default(), reduce)
}

/// Character level minus zone level. Negative means the player is behind.
pub fn level_gap(state: &TrackerState) -> Option<i64> {
    Some(state.level? - state.zone_level?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::events::EventKind;
    use chrono::NaiveDate;
    use serde_json::json;

    fn at(minute: u32) -> chrono::NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 8, 2)
            .unwrap()
            .and_hms_opt(12, minute, 0)
            .unwrap()
    }

    fn ev(minute: u32, kind: EventKind, payload: serde_json::Value) -> Event {
        Event {
            ts: at(minute),
            kind,
            payload,
        }
    }

    fn level_up(minute: u32, name: &str, level: i64) -> Event {
        ev(
            minute,
            EventKind::LevelUp,
            json!({"character": name, "ascendancy": "Sorceress", "level": level}),
        )
    }

    fn area(minute: u32, code: &str, level: i64) -> Event {
        ev(
            minute,
            EventKind::AreaEntered,
            json!({"code": code, "area_level": level}),
        )
    }

    #[test]
    fn level_up_sets_character_and_level() {
        let s = build_state(&[level_up(1, "Hero", 12)]);
        assert_eq!(s.character.as_deref(), Some("Hero"));
        assert_eq!(s.ascendancy.as_deref(), Some("Sorceress"));
        assert_eq!(s.level, Some(12));
        assert_eq!(s.character_confirmed_ts, Some(at(1)));
    }

    #[test]
    fn switching_character_clears_the_level() {
        // The new character's level is unknown until they level up. Keeping the
        // old one would show a confident, wrong level gap against the new zone.
        let events = vec![
            level_up(1, "Hero", 12),
            ev(
                2,
                EventKind::QuestReward,
                json!({"character": "Other", "reward": "+10% to Cold Resistance"}),
            ),
        ];
        let s = build_state(&events);
        assert_eq!(s.character.as_deref(), Some("Other"));
        assert_eq!(s.level, None);
    }

    #[test]
    fn returning_to_a_known_character_restores_their_ascendancy() {
        let events = vec![
            level_up(1, "Hero", 12),
            level_up(2, "Other", 5),
            ev(
                3,
                EventKind::QuestReward,
                json!({"character": "Hero", "reward": "+1 Charm Slot"}),
            ),
        ];
        let s = build_state(&events);
        assert_eq!(s.character.as_deref(), Some("Hero"));
        assert_eq!(s.ascendancy.as_deref(), Some("Sorceress"));
    }

    #[test]
    fn area_sets_zone_and_its_start_time() {
        let s = build_state(&[area(5, "G1_4", 4)]);
        assert_eq!(s.zone_code.as_deref(), Some("G1_4"));
        assert_eq!(s.zone_level, Some(4));
        assert_eq!(s.zone_since, Some(at(5)));
    }

    #[test]
    fn only_act_shaped_scenes_set_the_act() {
        let s = build_state(&[
            ev(
                1,
                EventKind::Scene,
                json!({"source": "Clearfell Encampment"}),
            ),
            ev(2, EventKind::Scene, json!({"source": "Act 2"})),
        ]);
        assert_eq!(s.act.as_deref(), Some("Act 2"));

        let s = build_state(&[ev(1, EventKind::Scene, json!({"source": "Atlas"}))]);
        assert_eq!(s.act, None);
    }

    #[test]
    fn rewards_accumulate_per_character() {
        let events = vec![
            ev(
                1,
                EventKind::QuestReward,
                json!({"character": "Hero", "reward": "a"}),
            ),
            ev(
                2,
                EventKind::QuestReward,
                json!({"character": "Hero", "reward": "b"}),
            ),
            ev(
                3,
                EventKind::QuestReward,
                json!({"character": "Other", "reward": "c"}),
            ),
        ];
        let s = build_state(&events);
        assert_eq!(
            s.rewards.get("Hero").unwrap(),
            &vec!["a".to_string(), "b".to_string()]
        );
        assert_eq!(s.rewards.get("Other").unwrap(), &vec!["c".to_string()]);
    }

    #[test]
    fn focus_is_tracked_both_ways() {
        let s = build_state(&[ev(1, EventKind::Focus, json!({"gained": true}))]);
        assert!(s.focused);
        let s = build_state(&[
            ev(1, EventKind::Focus, json!({"gained": true})),
            ev(2, EventKind::Focus, json!({"gained": false})),
        ]);
        assert!(!s.focused);
    }

    #[test]
    fn level_gap_needs_both_sides() {
        let s = build_state(&[level_up(1, "Hero", 10), area(2, "G1_4", 12)]);
        assert_eq!(level_gap(&s), Some(-2));

        let s = build_state(&[area(1, "G1_4", 12)]);
        assert_eq!(level_gap(&s), None);

        let s = build_state(&[level_up(1, "Hero", 10)]);
        assert_eq!(level_gap(&s), None);
    }

    #[test]
    fn last_ts_follows_every_event() {
        let s = build_state(&[
            level_up(1, "Hero", 10),
            ev(9, EventKind::Disconnect, json!({})),
        ]);
        assert_eq!(s.last_ts, Some(at(9)));
    }

    #[test]
    fn level_up_with_no_character_leaves_the_active_character_untouched() {
        let events = vec![
            level_up(1, "Hero", 12),
            ev(
                2,
                EventKind::LevelUp,
                json!({"ascendancy": "Witch", "level": 99}),
            ),
        ];
        let s = build_state(&events);
        assert_eq!(s.character.as_deref(), Some("Hero"));
        assert_eq!(s.level, Some(12));
        assert_eq!(s.character_confirmed_ts, Some(at(1)));
    }

    #[test]
    fn quest_reward_with_no_character_leaves_the_active_character_untouched() {
        let events = vec![
            level_up(1, "Hero", 12),
            ev(
                2,
                EventKind::QuestReward,
                json!({"reward": "+1 Charm Slot"}),
            ),
        ];
        let s = build_state(&events);
        assert_eq!(s.character.as_deref(), Some("Hero"));
        assert_eq!(s.level, Some(12));
        assert_eq!(s.character_confirmed_ts, Some(at(1)));
        assert!(s.rewards.is_empty());
    }

    #[test]
    fn malformed_level_leaves_the_previous_level_in_place() {
        let events = vec![
            level_up(1, "Hero", 12),
            ev(
                2,
                EventKind::LevelUp,
                json!({"character": "Hero", "ascendancy": "Sorceress", "level": "not-a-number"}),
            ),
        ];
        let s = build_state(&events);
        assert_eq!(s.character.as_deref(), Some("Hero"));
        assert_eq!(s.level, Some(12));
    }

    #[test]
    fn empty_history_is_an_empty_state() {
        let s = build_state(&[]);
        assert_eq!(s, TrackerState::default());
        assert_eq!(level_gap(&s), None);
    }
}
