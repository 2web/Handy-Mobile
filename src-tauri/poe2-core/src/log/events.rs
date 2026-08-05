//! The one shape of data that travels the whole pipeline.

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    AreaEntered,
    LevelUp,
    QuestReward,
    Scene,
    Focus,
    Disconnect,
    Slain,
}

impl EventKind {
    /// Stored in SQLite as text so a hand-run SELECT stays readable and adding
    /// a kind later renumbers nothing.
    pub fn as_str(&self) -> &'static str {
        match self {
            EventKind::AreaEntered => "area_entered",
            EventKind::LevelUp => "level_up",
            EventKind::QuestReward => "quest_reward",
            EventKind::Scene => "scene",
            EventKind::Focus => "focus",
            EventKind::Disconnect => "disconnect",
            EventKind::Slain => "slain",
        }
    }

    pub fn from_str_opt(raw: &str) -> Option<EventKind> {
        match raw {
            "area_entered" => Some(EventKind::AreaEntered),
            "level_up" => Some(EventKind::LevelUp),
            "quest_reward" => Some(EventKind::QuestReward),
            "scene" => Some(EventKind::Scene),
            "focus" => Some(EventKind::Focus),
            "disconnect" => Some(EventKind::Disconnect),
            "slain" => Some(EventKind::Slain),
            _ => None,
        }
    }
}

/// A single thing that happened, as read from the log.
///
/// The timestamp is naive on purpose: the game writes local time with no zone,
/// and attaching one would shift every duration this program computes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub ts: NaiveDateTime,
    pub kind: EventKind,
    pub payload: serde_json::Value,
}
