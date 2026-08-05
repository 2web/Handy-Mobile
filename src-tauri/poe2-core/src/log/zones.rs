//! The zone directory builds itself: the code comes from `Generating level`, the
//! name from the `Set Source` line that follows it moments later.

use chrono::NaiveDateTime;
use once_cell::sync::Lazy;
use regex::Regex;

use crate::log::events::{Event, EventKind};

/// A `Set Source` further than this from its zone belongs to some other zone.
pub const MAX_GAP_SECONDS: i64 = 60;

static ACT_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^Act \d+$").unwrap());

/// What the caller should write into the zone directory.
#[derive(Debug, Clone, PartialEq)]
pub struct ZoneUpdate {
    pub code: String,
    pub area_level: i64,
    pub name: Option<String>,
    pub act: Option<String>,
}

struct Pending {
    code: String,
    area_level: i64,
    ts: NaiveDateTime,
}

pub struct ZoneResolver {
    pending: Option<Pending>,
}

impl Default for ZoneResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl ZoneResolver {
    pub fn new() -> ZoneResolver {
        ZoneResolver { pending: None }
    }

    /// Returns the updates this event implies. Writing them is the caller's job,
    /// which keeps this module free of storage and its pairing rule testable.
    ///
    /// A name written here can later be overwritten by a different `Set Source`,
    /// and that is acceptable: the directory heals itself on the next correct
    /// observation, and nothing but the on-screen label depends on it — zone code
    /// and level come from the event stream, never from this table.
    pub fn feed(&mut self, event: &Event) -> Vec<ZoneUpdate> {
        match event.kind {
            EventKind::AreaEntered => {
                let (Some(code), Some(area_level)) = (
                    event.payload["code"].as_str(),
                    event.payload["area_level"].as_i64(),
                ) else {
                    return Vec::new();
                };
                self.pending = Some(Pending {
                    code: code.to_string(),
                    area_level,
                    ts: event.ts,
                });
                vec![ZoneUpdate {
                    code: code.to_string(),
                    area_level,
                    name: None,
                    act: None,
                }]
            }
            EventKind::Scene => {
                let Some(pending) = &self.pending else {
                    return Vec::new();
                };
                let gap = (event.ts - pending.ts).num_seconds();
                if gap < 0 {
                    return Vec::new();
                }
                if gap > MAX_GAP_SECONDS {
                    self.pending = None;
                    return Vec::new();
                }
                let Some(source) = event.payload["source"].as_str() else {
                    return Vec::new();
                };
                let is_act = ACT_RE.is_match(source);
                // The pending zone is deliberately NOT cleared here: one entry is
                // followed by several Set Source lines — the name and "Act N"
                // arrive separately, name first.
                vec![ZoneUpdate {
                    code: pending.code.clone(),
                    area_level: pending.area_level,
                    name: if is_act {
                        None
                    } else {
                        Some(source.to_string())
                    },
                    act: if is_act {
                        Some(source.to_string())
                    } else {
                        None
                    },
                }]
            }
            _ => Vec::new(),
        }
    }

    pub fn flush(&mut self) {
        self.pending = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::events::EventKind;
    use chrono::NaiveDate;
    use serde_json::json;

    fn at(second: u32) -> chrono::NaiveDateTime {
        // `and_hms_opt` rejects a seconds field >= 60, which `a_scene_after_the_window_is_ignored`
        // needs (MAX_GAP_SECONDS + 1). Adding a Duration instead gives the same NaiveDateTime for
        // every value used elsewhere in this file (all < 60) and rolls over correctly past it.
        NaiveDate::from_ymd_opt(2026, 8, 2)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap()
            + chrono::Duration::seconds(second as i64)
    }

    fn area(second: u32, code: &str, level: i64) -> Event {
        Event {
            ts: at(second),
            kind: EventKind::AreaEntered,
            payload: json!({"code": code, "area_level": level}),
        }
    }

    fn scene(second: u32, source: &str) -> Event {
        Event {
            ts: at(second),
            kind: EventKind::Scene,
            payload: json!({"source": source}),
        }
    }

    #[test]
    fn entering_a_zone_records_it_immediately() {
        let mut r = ZoneResolver::new();
        let updates = r.feed(&area(0, "G1_4", 4));
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].code, "G1_4");
        assert_eq!(updates[0].area_level, 4);
        assert_eq!(updates[0].name, None);
        assert_eq!(updates[0].act, None);
    }

    #[test]
    fn a_following_scene_names_the_zone() {
        let mut r = ZoneResolver::new();
        r.feed(&area(0, "G1_4", 4));
        let updates = r.feed(&scene(2, "Clearfell Encampment"));
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].code, "G1_4");
        assert_eq!(updates[0].name.as_deref(), Some("Clearfell Encampment"));
        assert_eq!(updates[0].act, None);
    }

    #[test]
    fn both_the_name_and_the_act_are_recorded() {
        // One zone entry is followed by several Set Source lines: the human name
        // and "Act N" arrive separately, name first. Clearing the pending zone on
        // the first match means the act is never recorded at all.
        let mut r = ZoneResolver::new();
        r.feed(&area(0, "G1_4", 4));
        let named = r.feed(&scene(1, "Clearfell Encampment"));
        let acted = r.feed(&scene(2, "Act 1"));
        assert_eq!(named[0].name.as_deref(), Some("Clearfell Encampment"));
        assert_eq!(named[0].act, None);
        assert_eq!(acted[0].act.as_deref(), Some("Act 1"));
        assert_eq!(acted[0].name, None);
    }

    #[test]
    fn a_scene_after_the_window_is_ignored() {
        let mut r = ZoneResolver::new();
        r.feed(&area(0, "G1_4", 4));
        let updates = r.feed(&scene(MAX_GAP_SECONDS as u32 + 1, "Somewhere Else"));
        assert!(updates.is_empty(), "the window has expired");
    }

    #[test]
    fn a_scene_before_any_zone_is_ignored() {
        let mut r = ZoneResolver::new();
        assert!(r.feed(&scene(0, "Clearfell Encampment")).is_empty());
    }

    #[test]
    fn a_new_zone_replaces_the_pending_one() {
        let mut r = ZoneResolver::new();
        r.feed(&area(0, "G1_4", 4));
        r.feed(&area(1, "G1_5", 5));
        let updates = r.feed(&scene(2, "Mud Burrow"));
        assert_eq!(
            updates[0].code, "G1_5",
            "the name belongs to the latest zone"
        );
    }

    #[test]
    fn flush_forgets_the_pending_zone() {
        let mut r = ZoneResolver::new();
        r.feed(&area(0, "G1_4", 4));
        r.flush();
        assert!(r.feed(&scene(1, "Clearfell Encampment")).is_empty());
    }
}
