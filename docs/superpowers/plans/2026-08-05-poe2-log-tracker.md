# Path of Exile 2 Log Tracker Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Handy reads the game's `Client.txt`, turns it into an immutable event log, and shows the player's live state — character, level, zone, act, time in zone, quest rewards — in a "Progress" tab beside the existing "Items" tab.

**Architecture:** All logic goes into the existing `poe2-core` crate under a new `log` module: `parser.rs` (lines to events), `tail.rs` (incremental reads and rotation), `state.rs` (folding events into state), `zones.rs` (a self-building zone directory). The `handy` crate gets `tracker.rs` — a polling thread that ingests and emits a Tauri event — plus three commands. The event log is append-only; `zones` and `characters` are derived and rebuildable from it.

**Tech Stack:** Rust 2021, `regex`, `once_cell`, `chrono`, `rusqlite` + `rusqlite_migration`, `serde`/`serde_json`, `specta`, Tauri 2.11. Frontend: React + TypeScript, i18next, `@tauri-apps/plugin-dialog` for the file picker.

## Global Constraints

- **No new third-party crates or npm packages.** Everything named above is already a dependency of `poe2-core`, `handy`, or the frontend.
- `poe2-core` contains **no Tauri types**. Anything needing an `AppHandle` lives in `src-tauri/src/poe2/`.
- **The event log is immutable.** Nothing updates or deletes a row in `events`. Derived tables are rebuilt, never patched.
- **Game files are opened read-only.** The program never writes to `Client.txt` or anything beside it.
- All file reads are **UTF-8 with invalid sequences replaced**, never strict.
- **Only these pre-existing files may be edited:** `src-tauri/poe2-core/src/lib.rs`, `src-tauri/poe2-core/src/store.rs`, `src-tauri/src/poe2/mod.rs`, `src-tauri/src/poe2/commands.rs`, `src-tauri/src/lib.rs`, `src-tauri/src/settings.rs`, `src/components/Sidebar.tsx`, `src/stores/settingsStore.ts`, `src/components/poe2/ItemsPage.tsx`, `src/i18n/locales/en/translation.json`, `src/i18n/locales/ru/translation.json`. Anything else is new.
- i18n keys go in **`en` and `ru` only**. `bun scripts/check-translations.ts` already fails on this fork's other 22 locales; that is not yours to fix.
- **Tests run in `poe2-core`:** `cd src-tauri/poe2-core && cargo test`, seconds. Never `cargo test` in `src-tauri` — that binary cannot start on this machine (`STATUS_ENTRYPOINT_NOT_FOUND`, from `transcribe-cpp`'s native DLLs). Use `cd src-tauri && cargo build` to check the `handy` crate compiles.
- Format with `cargo fmt` in both crates, then check `git status` and revert unrelated files the formatter touched — this repo has pre-existing drift in `actions.rs` and `tray.rs`.
- Frontend gates: `bun run lint` and `bunx tsc --noEmit`, both clean.
- Commit messages in English, `feat(poe2):` / `fix(poe2):` / `test(poe2):` style. No Co-Authored-By trailer.

---

## What already exists — use as is

`src-tauri/poe2-core/src/lib.rs` — `pub mod clipboard_watch; pub mod items; pub mod store;` plus `#[cfg(test)] mod acceptance;`.

`src-tauri/poe2-core/src/store.rs` — `const MIGRATIONS: [&str; 2]`, applied by `Migrations::new(MIGRATIONS.iter().map(|sql| M::up(sql)).collect())` inside `from_connection`, which also sets `conn.busy_timeout(Duration::from_secs(5))` before migrating. `Poe2Store { conn: Connection }` with `open(&Path)`, `in_memory()`, `add_item(&mut self, …)`, `item`, `items`, `raw_items`, `reparse_item(&mut self, …)`. Write methods take `&mut self` because they open a transaction.

`src-tauri/src/poe2/commands.rs` — `store_for(app: &AppHandle) -> Result<Poe2Store, String>` opening `poe2.db` in `crate::portable::app_data_dir(app)?`; commands `poe2_add_item`, `poe2_list_items`, `poe2_rebuild_items`, `change_poe2_enabled_setting`, `change_poe2_clipboard_watch_setting`; `RebuildResult { reparsed: u32, failed: u32 }`; `NOT_AN_ITEM_ERROR`.

`src-tauri/src/poe2/watcher.rs` — `spawn(app: AppHandle)` guarded by `static WATCHER_RUNNING: AtomicBool` with `compare_exchange`; a thread that persists for the process, skipping its read while the settings are off; emits `poe2://item-captured`.

`src-tauri/src/settings.rs` — `AppSettings` fields each with `#[serde(default = "default_…")]`, matching `default_*()` functions, and a struct literal in `get_default_settings()` that must also list every field.

`src/components/Sidebar.tsx` — `SECTIONS_CONFIG`, whose `poe2` entry currently points `component` at `ItemsPage` and is gated on `settings?.poe2_enabled ?? false`.

`src/components/poe2/ItemsPage.tsx` — listens for `poe2://item-captured` via `listen` from `@tauri-apps/api/event`, refetches with `commands.poe2ListItems()`, and follows the `result.status === "ok"` / `result.data` shape.

`src/stores/settingsStore.ts` — the `settingUpdaters` map. **A settings key missing from it silently fails to persist and only logs to the console.**

---

## File structure

| File | Responsibility |
|---|---|
| `src-tauri/poe2-core/src/log/mod.rs` | module wiring |
| `src-tauri/poe2-core/src/log/events.rs` | `Event`, `EventKind`, payload accessors |
| `src-tauri/poe2-core/src/log/parser.rs` | line → event; **every log regex** |
| `src-tauri/poe2-core/src/log/tail.rs` | incremental reads, fingerprint, rotation |
| `src-tauri/poe2-core/src/log/state.rs` | `TrackerState`, `reduce`, `build_state`, `level_gap` |
| `src-tauri/poe2-core/src/log/zones.rs` | pairing zone codes with their names |
| `src-tauri/poe2-core/src/log/fixtures/sample_client.txt` | real log excerpt, `include_str!` |
| `src-tauri/src/poe2/tracker.rs` | polling thread, ingestion, `poe2://state-changed` |
| `src/components/poe2/Poe2Page.tsx` | the two tabs |
| `src/components/poe2/ProgressTab.tsx` | live state display |

---

## Task 1: Events and the line parser

**Files:**
- Create: `src-tauri/poe2-core/src/log/mod.rs`, `log/events.rs`, `log/parser.rs`, `log/fixtures/sample_client.txt`
- Modify: `src-tauri/poe2-core/src/lib.rs`

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `log::events::EventKind` — enum `AreaEntered`, `LevelUp`, `QuestReward`, `Scene`, `Focus`, `Disconnect`, `Slain`; `serde(rename_all = "snake_case")`; `as_str(&self) -> &'static str`; `from_str_opt(&str) -> Option<EventKind>`
  - `log::events::Event { ts: NaiveDateTime, kind: EventKind, payload: serde_json::Value }`
  - `log::parser::parse_line(line: &str) -> Option<Event>`
  - `log::parser::clean_markup(text: &str) -> String`

Timestamps are `NaiveDateTime`: the game writes local time with no zone, and inventing one would shift every duration.

- [ ] **Step 1: Create the fixture**

Create `src-tauri/poe2-core/src/log/fixtures/sample_client.txt` with exactly these twelve lines:

```
2026/08/02 12:05:17 9674984 3ef231e0 [INFO Client 42864] : Kasablankee (Disciple of Varashta) is now level 61
2026/08/02 17:43:48 29985703 2caa2332 [DEBUG Client 23396] Generating level 59 area "P2_2" with seed 2251200547
2026/07/12 17:33:58 549544640 3ef231e0 [INFO Client 34520] : Kasablankee has received +10% to [Resistances|Cold Resistance].
2026/07/17 08:07:36 116035906 3ef231e0 [INFO Client 25780] : Kasablankee has received +1 [Charm] Slot.
2026/08/03 22:06:31 132140562 7fbd1225 [INFO Client 706388] [SCENE] Set Source [(null)]
2026/08/03 21:50:15 131165015 2caa2332 [INFO Client 706388] [SCENE] Set Source [Clearfell Encampment]
2026/08/03 22:08:04 132233359 5288531e [INFO Client 706388] [WINDOW] Gained focus
2026/08/03 21:00:00 131000000 5288531e [INFO Client 706388] [WINDOW] Lost focus
2026/08/03 22:06:31 132140546 2d8e8dd7 [INFO Client 706388] Abnormal disconnect: An unexpected disconnection occurred.
2026/07/12 13:51:02 536168359 3ef231e0 [INFO Client 55192] : MaruniaKazaMorta has been slain.
2026/08/03 22:00:41 131790703 3ef231e0 [INFO Client 706388] #GorillaGripMyBussy: Kasablankee has received +30 to [Spirit|Spirit].
2026/08/03 22:00:38 131787843 3ef231e0 [INFO Client 706388] [SOUND] Device List changed...
```

Line 11 is the trap that makes this fixture worth having: it is a **chat message** whose text looks exactly like a quest reward. A player wrote it in the game's chat. It must not become a `quest_reward` event, and the guard is that a real reward line's message begins with `": "` while this one begins with a channel name.

- [ ] **Step 2: Write the failing tests**

Create `src-tauri/poe2-core/src/log/parser.rs` containing only this test module for now:

```rust
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
        assert!(empty.is_none(), "(null) carries no information about a zone");
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
```

- [ ] **Step 3: Run the tests to verify they fail**

```bash
cd src-tauri/poe2-core && cargo test log::
```

Expected: compilation error — `file not found for module log`.

- [ ] **Step 4: Write the event type**

Create `src-tauri/poe2-core/src/log/mod.rs`:

```rust
//! Reading Path of Exile 2's `Client.txt` and deriving player state from it.

pub mod events;
pub mod parser;
```

Create `src-tauri/poe2-core/src/log/events.rs`:

```rust
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
```

- [ ] **Step 5: Write the parser**

Insert above the `#[cfg(test)]` block in `src-tauri/poe2-core/src/log/parser.rs`:

```rust
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
static SLAIN_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^: (?P<name>\S+) has been slain\.").unwrap());
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
```

Register the module: add `pub mod log;` to `src-tauri/poe2-core/src/lib.rs`, beside the existing `pub mod items;`.

- [ ] **Step 6: Run the tests to verify they pass**

```bash
cd src-tauri/poe2-core && cargo test
```

Expected: PASS — the 67 existing tests plus 10 new ones.

- [ ] **Step 7: Format and commit**

```bash
cd src-tauri/poe2-core && cargo fmt
cd ../.. && git add src-tauri/poe2-core
git commit -m "feat(poe2): parse Client.txt lines into events"
```

---

## Task 2: Tailing the log and detecting rotation

**Files:**
- Create: `src-tauri/poe2-core/src/log/tail.rs`
- Modify: `src-tauri/poe2-core/src/log/mod.rs`

**Interfaces:**
- Consumes: nothing from Task 1 — this module never interprets a line's contents.
- Produces:
  - `log::tail::FINGERPRINT_BYTES: usize` = 512
  - `log::tail::LogTail { pub offset: u64, pub fingerprint: Option<String>, pub rotated: bool }` with `new(path: &Path, offset: u64, fingerprint: Option<String>) -> LogTail` and `read_new(&mut self) -> Vec<(u64, String)>` returning `(offset of the line's end, line)`

- [ ] **Step 1: Write the failing tests**

Create `src-tauri/poe2-core/src/log/tail.rs` with only this test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    /// A unique temp file per test: these run in parallel and a shared name
    /// would have them clobbering each other's log.
    fn temp_path(name: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("poe2-tail-{name}-{}.txt", std::process::id()));
        let _ = fs::remove_file(&p);
        p
    }

    fn append(path: &std::path::Path, text: &str) {
        let mut f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap();
        f.write_all(text.as_bytes()).unwrap();
    }

    #[test]
    fn reads_only_complete_lines() {
        let path = temp_path("complete");
        append(&path, "first\nsecond\npartial");
        let mut tail = LogTail::new(&path, 0, None);
        let lines: Vec<String> = tail.read_new().into_iter().map(|(_, l)| l).collect();
        assert_eq!(lines, vec!["first", "second"]);

        // The partial line arrives only once its newline does.
        append(&path, " finished\n");
        let lines: Vec<String> = tail.read_new().into_iter().map(|(_, l)| l).collect();
        assert_eq!(lines, vec!["partial finished"]);
        fs::remove_file(&path).ok();
    }

    #[test]
    fn offsets_point_past_each_line() {
        let path = temp_path("offsets");
        append(&path, "ab\ncd\n");
        let mut tail = LogTail::new(&path, 0, None);
        let offsets: Vec<u64> = tail.read_new().into_iter().map(|(o, _)| o).collect();
        assert_eq!(offsets, vec![3, 6]);
        assert_eq!(tail.offset, 6);
        fs::remove_file(&path).ok();
    }

    #[test]
    fn nothing_new_yields_nothing() {
        let path = temp_path("nothing");
        append(&path, "one\n");
        let mut tail = LogTail::new(&path, 0, None);
        assert_eq!(tail.read_new().len(), 1);
        assert!(tail.read_new().is_empty());
        assert!(!tail.rotated);
        fs::remove_file(&path).ok();
    }

    #[test]
    fn missing_file_is_not_an_error() {
        let path = temp_path("missing");
        let mut tail = LogTail::new(&path, 0, None);
        assert!(tail.read_new().is_empty());
        assert!(!tail.rotated);
    }

    #[test]
    fn truncation_is_detected_and_restarts_from_zero() {
        let path = temp_path("truncate");
        append(&path, "aaaa\nbbbb\n");
        let mut tail = LogTail::new(&path, 0, None);
        tail.read_new();
        assert_eq!(tail.offset, 10);

        fs::write(&path, "cc\n").unwrap();
        let lines: Vec<String> = tail.read_new().into_iter().map(|(_, l)| l).collect();
        assert!(tail.rotated, "a shorter file must read as rotation");
        assert_eq!(lines, vec!["cc"]);
        fs::remove_file(&path).ok();
    }

    #[test]
    fn replacement_of_the_same_length_is_caught_by_the_fingerprint() {
        // Size alone cannot see this: the new file is exactly as long as what we
        // had already read. Only the fingerprint of its head differs.
        let path = temp_path("replace");
        let old = "A".repeat(FINGERPRINT_BYTES) + "\n";
        let new = "B".repeat(FINGERPRINT_BYTES) + "\n";
        fs::write(&path, &old).unwrap();
        let mut tail = LogTail::new(&path, 0, None);
        tail.read_new();
        assert!(!tail.rotated);

        fs::write(&path, &new).unwrap();
        tail.read_new();
        assert!(tail.rotated, "same size, different content, must be rotation");
        fs::remove_file(&path).ok();
    }

    #[test]
    fn a_file_shorter_than_the_fingerprint_gets_none() {
        let path = temp_path("short");
        append(&path, "tiny\n");
        let mut tail = LogTail::new(&path, 0, None);
        tail.read_new();
        // While the file is this short, appending would change any hash of its
        // head, and a changing hash would read as endless rotation.
        assert_eq!(tail.fingerprint, None);
        append(&path, "more\n");
        tail.read_new();
        assert!(!tail.rotated, "growth of a short file is not rotation");
        fs::remove_file(&path).ok();
    }

    #[test]
    fn invalid_utf8_is_replaced_not_fatal() {
        let path = temp_path("utf8");
        fs::write(&path, b"good\n\xff\xfe bad\n").unwrap();
        let mut tail = LogTail::new(&path, 0, None);
        let lines: Vec<String> = tail.read_new().into_iter().map(|(_, l)| l).collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "good");
        assert!(lines[1].contains("bad"));
        fs::remove_file(&path).ok();
    }

    #[test]
    fn carriage_returns_are_stripped() {
        let path = temp_path("crlf");
        append(&path, "windows\r\nline\r\n");
        let mut tail = LogTail::new(&path, 0, None);
        let lines: Vec<String> = tail.read_new().into_iter().map(|(_, l)| l).collect();
        assert_eq!(lines, vec!["windows", "line"]);
        fs::remove_file(&path).ok();
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cd src-tauri/poe2-core && cargo test log::tail
```

Expected: compilation error — `cannot find type LogTail`.

- [ ] **Step 3: Write the implementation**

Insert above the `#[cfg(test)]` block in `src-tauri/poe2-core/src/log/tail.rs`:

```rust
//! Incremental reading of a growing file. Line contents are never interpreted.

use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// A hash of this many leading bytes distinguishes a new `Client.txt` from the
/// previous one even when the new file is longer. Truncation is not caught this
/// way — size handles that.
pub const FINGERPRINT_BYTES: usize = 512;

/// Hands out new complete lines and reports that the file was rotated.
///
/// Detecting rotation is this type's job; deciding what to do about it — start a
/// new generation in the store — belongs to the caller.
pub struct LogTail {
    path: PathBuf,
    pub offset: u64,
    pub fingerprint: Option<String>,
    pub rotated: bool,
}

impl LogTail {
    pub fn new(path: &Path, offset: u64, fingerprint: Option<String>) -> LogTail {
        LogTail {
            path: path.to_path_buf(),
            offset,
            fingerprint,
            rotated: false,
        }
    }

    fn head_fingerprint(head: &[u8]) -> Option<String> {
        // While the file is shorter than FINGERPRINT_BYTES its hash changes with
        // every append, and a changing hash would read as endless rotation.
        if head.len() < FINGERPRINT_BYTES {
            return None;
        }
        Some(format!("{:x}", Sha256::digest(head)))
    }

    fn is_rotated(&self, size: u64, fingerprint: &Option<String>) -> bool {
        if size < self.offset {
            return true;
        }
        // A replacement longer than the old offset is invisible by size alone.
        match (fingerprint, &self.fingerprint) {
            (Some(new), Some(old)) => new != old,
            _ => false,
        }
    }

    /// New complete lines as `(offset just past the line, line)`.
    ///
    /// Any I/O error yields an empty result rather than propagating: the game can
    /// delete or recreate the file between the metadata call and the read, and
    /// skipping one poll is safer than killing the update thread.
    pub fn read_new(&mut self) -> Vec<(u64, String)> {
        self.rotated = false;

        let Ok(metadata) = std::fs::metadata(&self.path) else {
            return Vec::new();
        };
        let size = metadata.len();

        let Ok(mut file) = File::open(&self.path) else {
            return Vec::new();
        };

        let mut head = vec![0u8; FINGERPRINT_BYTES];
        let read = match file.read(&mut head) {
            Ok(n) => n,
            Err(_) => return Vec::new(),
        };
        head.truncate(read);
        let fingerprint = Self::head_fingerprint(&head);

        if self.is_rotated(size, &fingerprint) {
            self.offset = 0;
            self.rotated = true;
        }
        // Assigned unconditionally, None included: holding on to the previous
        // file's fingerprint would make a later, longer file look like yet
        // another rotation.
        self.fingerprint = fingerprint;

        if size <= self.offset {
            return Vec::new();
        }
        if file.seek(SeekFrom::Start(self.offset)).is_err() {
            return Vec::new();
        }
        let mut data = Vec::with_capacity((size - self.offset) as usize);
        if file.take(size - self.offset).read_to_end(&mut data).is_err() {
            return Vec::new();
        }

        let Some(last_newline) = data.iter().rposition(|b| *b == b'\n') else {
            // Not one complete line yet — wait for the game to finish writing.
            return Vec::new();
        };
        let complete = &data[..=last_newline];

        let mut result = Vec::new();
        let mut position = self.offset;
        for raw in complete.split(|b| *b == b'\n') {
            if position + raw.len() as u64 >= self.offset + complete.len() as u64 {
                break;
            }
            position += raw.len() as u64 + 1;
            let line = String::from_utf8_lossy(raw).trim_end_matches('\r').to_string();
            result.push((position, line));
        }
        self.offset += complete.len() as u64;
        result
    }
}
```

Add `pub mod tail;` to `src-tauri/poe2-core/src/log/mod.rs`.

- [ ] **Step 4: Run the tests to verify they pass**

```bash
cd src-tauri/poe2-core && cargo test
```

Expected: PASS — 9 new tests on top of the previous total.

- [ ] **Step 5: Format and commit**

```bash
cd src-tauri/poe2-core && cargo fmt
cd ../.. && git add src-tauri/poe2-core
git commit -m "feat(poe2): tail Client.txt and detect rotation by fingerprint"
```

---

## Task 3: Folding events into state

**Files:**
- Create: `src-tauri/poe2-core/src/log/state.rs`
- Modify: `src-tauri/poe2-core/src/log/mod.rs`

**Interfaces:**
- Consumes: `Event`, `EventKind` from Task 1.
- Produces:
  - `log::state::TrackerState` — fields exactly as the spec lists, deriving `Debug, Clone, Default, PartialEq, Serialize, Deserialize`
  - `log::state::reduce(state: TrackerState, event: &Event) -> TrackerState`
  - `log::state::build_state<'a>(events: impl IntoIterator<Item = &'a Event>) -> TrackerState`
  - `log::state::level_gap(state: &TrackerState) -> Option<i64>`

**`TrackerState` deliberately does not derive `specta::Type`.** Its timestamps are
`NaiveDateTime`, and `specta` only knows that type when built with its chrono feature, which
this project does not enable. Rather than turning a feature flag on across the workspace to
satisfy one struct, Task 6 flattens what the frontend needs into its own type with the
timestamps rendered as strings. The internal type keeps real dates, where arithmetic on them
belongs.

- [ ] **Step 1: Write the failing tests**

Create `src-tauri/poe2-core/src/log/state.rs` with only this test module:

```rust
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
        Event { ts: at(minute), kind, payload }
    }

    fn level_up(minute: u32, name: &str, level: i64) -> Event {
        ev(minute, EventKind::LevelUp,
           json!({"character": name, "ascendancy": "Sorceress", "level": level}))
    }

    fn area(minute: u32, code: &str, level: i64) -> Event {
        ev(minute, EventKind::AreaEntered, json!({"code": code, "area_level": level}))
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
            ev(2, EventKind::QuestReward,
               json!({"character": "Other", "reward": "+10% to Cold Resistance"})),
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
            ev(3, EventKind::QuestReward,
               json!({"character": "Hero", "reward": "+1 Charm Slot"})),
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
            ev(1, EventKind::Scene, json!({"source": "Clearfell Encampment"})),
            ev(2, EventKind::Scene, json!({"source": "Act 2"})),
        ]);
        assert_eq!(s.act.as_deref(), Some("Act 2"));

        let s = build_state(&[ev(1, EventKind::Scene, json!({"source": "Atlas"}))]);
        assert_eq!(s.act, None);
    }

    #[test]
    fn rewards_accumulate_per_character() {
        let events = vec![
            ev(1, EventKind::QuestReward, json!({"character": "Hero", "reward": "a"})),
            ev(2, EventKind::QuestReward, json!({"character": "Hero", "reward": "b"})),
            ev(3, EventKind::QuestReward, json!({"character": "Other", "reward": "c"})),
        ];
        let s = build_state(&events);
        assert_eq!(s.rewards.get("Hero").unwrap(), &vec!["a".to_string(), "b".to_string()]);
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
        let s = build_state(&[level_up(1, "Hero", 10), ev(9, EventKind::Disconnect, json!({}))]);
        assert_eq!(s.last_ts, Some(at(9)));
    }

    #[test]
    fn empty_history_is_an_empty_state() {
        let s = build_state(&[]);
        assert_eq!(s, TrackerState::default());
        assert_eq!(level_gap(&s), None);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cd src-tauri/poe2-core && cargo test log::state
```

Expected: compilation error — `cannot find type TrackerState`.

- [ ] **Step 3: Write the implementation**

Insert above the `#[cfg(test)]` block in `src-tauri/poe2-core/src/log/state.rs`:

```rust
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
            let name = event.payload["character"].as_str().unwrap_or_default().to_string();
            let ascendancy = event.payload["ascendancy"].as_str().unwrap_or_default().to_string();
            next.known_characters.insert(name.clone(), ascendancy.clone());
            switch_character(&mut next, &name);
            next.character = Some(name);
            next.ascendancy = Some(ascendancy);
            next.level = event.payload["level"].as_i64();
            next.character_confirmed_ts = Some(event.ts);
        }
        EventKind::QuestReward => {
            let name = event.payload["character"].as_str().unwrap_or_default().to_string();
            switch_character(&mut next, &name);
            if let Some(reward) = event.payload["reward"].as_str() {
                next.rewards.entry(name.clone()).or_default().push(reward.to_string());
            }
            next.character = Some(name);
            next.character_confirmed_ts = Some(event.ts);
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
```

Add `pub mod state;` to `src-tauri/poe2-core/src/log/mod.rs`.

- [ ] **Step 4: Run the tests to verify they pass**

```bash
cd src-tauri/poe2-core && cargo test
```

Expected: PASS — 10 new tests on top of the previous total.

- [ ] **Step 5: Format and commit**

```bash
cd src-tauri/poe2-core && cargo fmt
cd ../.. && git add src-tauri/poe2-core
git commit -m "feat(poe2): fold log events into tracker state"
```

---

## Task 4: The zone directory

**Files:**
- Create: `src-tauri/poe2-core/src/log/zones.rs`
- Modify: `src-tauri/poe2-core/src/log/mod.rs`

**Interfaces:**
- Consumes: `Event`, `EventKind` from Task 1.
- Produces:
  - `log::zones::MAX_GAP_SECONDS: i64` = 60
  - `log::zones::ZoneUpdate { code: String, area_level: i64, name: Option<String>, act: Option<String> }`
  - `log::zones::ZoneResolver::new() -> ZoneResolver`, `feed(&mut self, event: &Event) -> Vec<ZoneUpdate>`, `flush(&mut self)`

`feed` returns the updates to apply rather than writing them itself: that keeps this module free of storage and makes the pairing rule testable on its own.

- [ ] **Step 1: Write the failing tests**

Create `src-tauri/poe2-core/src/log/zones.rs` with only this test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::events::EventKind;
    use chrono::NaiveDate;
    use serde_json::json;

    fn at(second: u32) -> chrono::NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 8, 2).unwrap().and_hms_opt(12, 0, second).unwrap()
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
        assert_eq!(updates[0].code, "G1_5", "the name belongs to the latest zone");
    }

    #[test]
    fn flush_forgets_the_pending_zone() {
        let mut r = ZoneResolver::new();
        r.feed(&area(0, "G1_4", 4));
        r.flush();
        assert!(r.feed(&scene(1, "Clearfell Encampment")).is_empty());
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cd src-tauri/poe2-core && cargo test log::zones
```

Expected: compilation error — `cannot find type ZoneResolver`.

- [ ] **Step 3: Write the implementation**

Insert above the `#[cfg(test)]` block in `src-tauri/poe2-core/src/log/zones.rs`:

```rust
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
                    name: if is_act { None } else { Some(source.to_string()) },
                    act: if is_act { Some(source.to_string()) } else { None },
                }]
            }
            _ => Vec::new(),
        }
    }

    pub fn flush(&mut self) {
        self.pending = None;
    }
}
```

Add `pub mod zones;` to `src-tauri/poe2-core/src/log/mod.rs`.

- [ ] **Step 4: Run the tests to verify they pass**

```bash
cd src-tauri/poe2-core && cargo test
```

Expected: PASS — 7 new tests on top of the previous total.

- [ ] **Step 5: Format and commit**

```bash
cd src-tauri/poe2-core && cargo fmt
cd ../.. && git add src-tauri/poe2-core
git commit -m "feat(poe2): pair zone codes with the names that follow them"
```

---

## Task 5: Event, zone and character storage

**Files:**
- Modify: `src-tauri/poe2-core/src/store.rs`

**Interfaces:**
- Consumes: `Event`, `EventKind` from Task 1; `ZoneUpdate` from Task 4.
- Produces, on `Poe2Store`:
  - `add_event(&mut self, event: &Event, file_offset: i64, generation: i64) -> anyhow::Result<bool>` — false means it was a duplicate
  - `ingest_batch(&mut self, batch: &IngestBatch) -> anyhow::Result<u32>` — everything one poll produced, in **one transaction**; returns how many events were new
  - `IngestBatch { pub events: Vec<(Event, i64)>, pub generation: i64, pub zones: Vec<ZoneUpdate>, pub characters: Vec<(String, Option<String>, Option<NaiveDateTime>)>, pub meta: Vec<(String, String)> }`
  - `events(&self) -> anyhow::Result<Vec<Event>>` — insertion order
  - `apply_zone_update(&mut self, update: &ZoneUpdate) -> anyhow::Result<()>`
  - `zones(&self) -> anyhow::Result<Vec<ZoneRow>>` where `ZoneRow { code: String, name: Option<String>, act: Option<String>, area_level: Option<i64> }` derives `Serialize`, `Type`
  - `upsert_character(&mut self, name: &str, ascendancy: Option<&str>, ts: Option<NaiveDateTime>) -> anyhow::Result<()>`
  - `characters(&self) -> anyhow::Result<Vec<CharacterRow>>` where `CharacterRow { name: String, ascendancy: Option<String>, last_seen_ts: Option<String> }` derives `Serialize`, `Type`
  - `get_meta(&self, key: &str) -> anyhow::Result<Option<String>>`, `set_meta(&mut self, key: &str, value: &str) -> anyhow::Result<()>`
  - `clear_derived(&mut self) -> anyhow::Result<()>` — empties `zones` and `characters`, never `events`

- [ ] **Step 1: Write the failing tests**

Append to the existing `#[cfg(test)] mod tests` in `src-tauri/poe2-core/src/store.rs`:

```rust
    use crate::log::events::{Event, EventKind};
    use crate::log::zones::ZoneUpdate;
    use chrono::NaiveDate;
    use serde_json::json;

    fn event_at(minute: u32, kind: EventKind) -> Event {
        Event {
            ts: NaiveDate::from_ymd_opt(2026, 8, 2)
                .unwrap()
                .and_hms_opt(12, minute, 0)
                .unwrap(),
            kind,
            payload: json!({"n": minute}),
        }
    }

    #[test]
    fn events_round_trip_in_insertion_order() {
        let mut s = store();
        for i in 0..3u32 {
            assert!(s
                .add_event(&event_at(i, EventKind::Focus), i as i64 * 10, 0)
                .unwrap());
        }
        let read = s.events().unwrap();
        assert_eq!(read.len(), 3);
        assert_eq!(read[0].payload["n"], 0);
        assert_eq!(read[2].payload["n"], 2);
        assert_eq!(read[0].kind, EventKind::Focus);
    }

    #[test]
    fn the_same_offset_in_one_generation_is_a_duplicate() {
        let mut s = store();
        assert!(s.add_event(&event_at(1, EventKind::Focus), 100, 0).unwrap());
        assert!(!s.add_event(&event_at(1, EventKind::Focus), 100, 0).unwrap());
        assert_eq!(s.events().unwrap().len(), 1);
    }

    #[test]
    fn the_same_offset_in_a_new_generation_is_kept() {
        // After the game truncates Client.txt, byte offsets start over and
        // repeat. Keyed on the offset alone, every event of the new generation
        // would be discarded as a duplicate.
        let mut s = store();
        assert!(s.add_event(&event_at(1, EventKind::Focus), 100, 0).unwrap());
        assert!(s.add_event(&event_at(2, EventKind::Focus), 100, 1).unwrap());
        assert_eq!(s.events().unwrap().len(), 2);
    }

    #[test]
    fn zone_name_and_act_accumulate_without_erasing_each_other() {
        let mut s = store();
        s.apply_zone_update(&ZoneUpdate {
            code: "G1_4".into(), area_level: 4, name: None, act: None,
        }).unwrap();
        s.apply_zone_update(&ZoneUpdate {
            code: "G1_4".into(), area_level: 4, name: Some("Clearfell".into()), act: None,
        }).unwrap();
        s.apply_zone_update(&ZoneUpdate {
            code: "G1_4".into(), area_level: 4, name: None, act: Some("Act 1".into()),
        }).unwrap();

        let zones = s.zones().unwrap();
        assert_eq!(zones.len(), 1);
        assert_eq!(zones[0].name.as_deref(), Some("Clearfell"));
        assert_eq!(zones[0].act.as_deref(), Some("Act 1"));
        assert_eq!(zones[0].area_level, Some(4));
    }

    #[test]
    fn a_known_character_keeps_its_ascendancy_when_seen_without_one() {
        let mut s = store();
        let ts = NaiveDate::from_ymd_opt(2026, 8, 2).unwrap().and_hms_opt(12, 0, 0).unwrap();
        s.upsert_character("Hero", Some("Sorceress"), Some(ts)).unwrap();
        s.upsert_character("Hero", None, Some(ts)).unwrap();
        let chars = s.characters().unwrap();
        assert_eq!(chars.len(), 1);
        assert_eq!(chars[0].ascendancy.as_deref(), Some("Sorceress"));
    }

    #[test]
    fn meta_round_trips() {
        let mut s = store();
        assert_eq!(s.get_meta("offset").unwrap(), None);
        s.set_meta("offset", "1234").unwrap();
        assert_eq!(s.get_meta("offset").unwrap().as_deref(), Some("1234"));
        s.set_meta("offset", "5678").unwrap();
        assert_eq!(s.get_meta("offset").unwrap().as_deref(), Some("5678"));
    }

    #[test]
    fn a_batch_lands_in_one_transaction() {
        let mut s = store();
        let ts = NaiveDate::from_ymd_opt(2026, 8, 2).unwrap().and_hms_opt(12, 0, 0).unwrap();
        let batch = IngestBatch {
            events: vec![
                (event_at(1, EventKind::Focus), 10),
                (event_at(2, EventKind::Focus), 20),
            ],
            generation: 0,
            zones: vec![ZoneUpdate {
                code: "G1_4".into(), area_level: 4, name: Some("Clearfell".into()), act: None,
            }],
            characters: vec![("Hero".into(), Some("Sorceress".into()), Some(ts))],
            meta: vec![("log_offset".into(), "20".into())],
        };
        assert_eq!(s.ingest_batch(&batch).unwrap(), 2);
        assert_eq!(s.events().unwrap().len(), 2);
        assert_eq!(s.zones().unwrap().len(), 1);
        assert_eq!(s.characters().unwrap().len(), 1);
        assert_eq!(s.get_meta("log_offset").unwrap().as_deref(), Some("20"));
    }

    #[test]
    fn a_batch_counts_only_the_events_that_were_new() {
        let mut s = store();
        let first = IngestBatch {
            events: vec![(event_at(1, EventKind::Focus), 10)],
            generation: 0,
            zones: vec![],
            characters: vec![],
            meta: vec![],
        };
        assert_eq!(s.ingest_batch(&first).unwrap(), 1);
        // Same generation, same offset: already stored.
        assert_eq!(s.ingest_batch(&first).unwrap(), 0);
        assert_eq!(s.events().unwrap().len(), 1);
    }

    #[test]
    fn clear_derived_leaves_the_event_log_alone() {
        let mut s = store();
        s.add_event(&event_at(1, EventKind::Focus), 10, 0).unwrap();
        s.apply_zone_update(&ZoneUpdate {
            code: "G1_4".into(), area_level: 4, name: Some("Clearfell".into()), act: None,
        }).unwrap();
        s.upsert_character("Hero", Some("Sorceress"), None).unwrap();

        s.clear_derived().unwrap();

        assert!(s.zones().unwrap().is_empty());
        assert!(s.characters().unwrap().is_empty());
        assert_eq!(s.events().unwrap().len(), 1, "the event log is immutable");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cd src-tauri/poe2-core && cargo test store::
```

Expected: compilation error — `no method named add_event`.

- [ ] **Step 3: Add the migrations**

In `src-tauri/poe2-core/src/store.rs`, change `const MIGRATIONS: [&str; 2]` to `[&str; 5]` and append these three entries after the existing two. **Order matters** — `rusqlite_migration` tracks position, so never insert in the middle:

```rust
    "CREATE TABLE IF NOT EXISTS events (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        ts TEXT NOT NULL,
        kind TEXT NOT NULL,
        payload TEXT NOT NULL,
        file_offset INTEGER NOT NULL,
        generation INTEGER NOT NULL DEFAULT 0,
        UNIQUE(generation, file_offset)
    );",
    "CREATE TABLE IF NOT EXISTS zones (
        code TEXT PRIMARY KEY,
        name TEXT,
        act TEXT,
        area_level INTEGER
    );",
    "CREATE TABLE IF NOT EXISTS characters (
        name TEXT PRIMARY KEY,
        ascendancy TEXT,
        last_seen_ts TEXT
    );",
```

The `meta` table is not new to this plan — add it as part of the events migration entry above by making that entry create both tables in one string:

```rust
    "CREATE TABLE IF NOT EXISTS events (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        ts TEXT NOT NULL,
        kind TEXT NOT NULL,
        payload TEXT NOT NULL,
        file_offset INTEGER NOT NULL,
        generation INTEGER NOT NULL DEFAULT 0,
        UNIQUE(generation, file_offset)
    );
    CREATE TABLE IF NOT EXISTS meta (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );",
```

- [ ] **Step 4: Write the methods**

Add to `impl Poe2Store` in `src-tauri/poe2-core/src/store.rs`:

```rust
    /// Appends an event. False means this (generation, offset) was already stored.
    ///
    /// The key is the pair, not the offset alone: after the game truncates
    /// Client.txt, byte offsets start over and repeat, and keyed on the offset
    /// alone every event of the new generation would be discarded as a duplicate.
    pub fn add_event(
        &mut self,
        event: &Event,
        file_offset: i64,
        generation: i64,
    ) -> Result<bool> {
        let changed = self.conn.execute(
            "INSERT OR IGNORE INTO events (ts, kind, payload, file_offset, generation)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                event.ts.format("%Y-%m-%dT%H:%M:%S").to_string(),
                event.kind.as_str(),
                serde_json::to_string(&event.payload)?,
                file_offset,
                generation,
            ],
        )?;
        Ok(changed > 0)
    }

    pub fn events(&self) -> Result<Vec<Event>> {
        let mut stmt = self
            .conn
            .prepare("SELECT ts, kind, payload FROM events ORDER BY id")?;
        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<(String, String, String)>>>()?;

        let mut events = Vec::with_capacity(rows.len());
        for (ts, kind, payload) in rows {
            // A row whose kind or timestamp no longer parses is skipped rather
            // than failing the whole read: the log itself is the source of truth
            // and can be re-ingested, but a single bad row must not blind the
            // tracker to every other event.
            let (Ok(ts), Some(kind)) = (
                NaiveDateTime::parse_from_str(&ts, "%Y-%m-%dT%H:%M:%S"),
                EventKind::from_str_opt(&kind),
            ) else {
                continue;
            };
            events.push(Event {
                ts,
                kind,
                payload: serde_json::from_str(&payload).unwrap_or(serde_json::Value::Null),
            });
        }
        Ok(events)
    }

    /// COALESCE keeps an already-known name or act from being blanked by a later
    /// observation that carries only the other one.
    pub fn apply_zone_update(&mut self, update: &ZoneUpdate) -> Result<()> {
        self.conn.execute(
            "INSERT INTO zones (code, name, act, area_level) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(code) DO UPDATE SET
                name = COALESCE(excluded.name, zones.name),
                act = COALESCE(excluded.act, zones.act),
                area_level = excluded.area_level",
            params![update.code, update.name, update.act, update.area_level],
        )?;
        Ok(())
    }

    pub fn zones(&self) -> Result<Vec<ZoneRow>> {
        let mut stmt = self
            .conn
            .prepare("SELECT code, name, act, area_level FROM zones ORDER BY code")?;
        let rows = stmt
            .query_map([], |row| {
                Ok(ZoneRow {
                    code: row.get(0)?,
                    name: row.get(1)?,
                    act: row.get(2)?,
                    area_level: row.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<ZoneRow>>>()?;
        Ok(rows)
    }

    pub fn upsert_character(
        &mut self,
        name: &str,
        ascendancy: Option<&str>,
        ts: Option<NaiveDateTime>,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO characters (name, ascendancy, last_seen_ts) VALUES (?1, ?2, ?3)
             ON CONFLICT(name) DO UPDATE SET
                ascendancy = COALESCE(excluded.ascendancy, characters.ascendancy),
                last_seen_ts = COALESCE(excluded.last_seen_ts, characters.last_seen_ts)",
            params![
                name,
                ascendancy,
                ts.map(|t| t.format("%Y-%m-%dT%H:%M:%S").to_string()),
            ],
        )?;
        Ok(())
    }

    pub fn characters(&self) -> Result<Vec<CharacterRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT name, ascendancy, last_seen_ts FROM characters ORDER BY last_seen_ts DESC",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(CharacterRow {
                    name: row.get(0)?,
                    ascendancy: row.get(1)?,
                    last_seen_ts: row.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<CharacterRow>>>()?;
        Ok(rows)
    }

    pub fn get_meta(&self, key: &str) -> Result<Option<String>> {
        let value = self
            .conn
            .query_row("SELECT value FROM meta WHERE key = ?1", params![key], |row| {
                row.get(0)
            })
            .optional()?;
        Ok(value)
    }

    pub fn set_meta(&mut self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    /// Everything one poll produced, written in a single transaction.
    ///
    /// This is not an optimisation. Committing per event turned the cold import
    /// of a 27 MB log into ninety seconds, almost all of it waiting on the disk.
    /// It also makes a poll atomic: the read offset in `meta` advances only if
    /// the events it covers were stored, so a crash mid-write cannot skip a
    /// stretch of the log forever.
    pub fn ingest_batch(&mut self, batch: &IngestBatch) -> Result<u32> {
        let tx = self.conn.transaction()?;
        let mut added = 0u32;

        {
            let mut stmt = tx.prepare(
                "INSERT OR IGNORE INTO events (ts, kind, payload, file_offset, generation)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for (event, file_offset) in &batch.events {
                let changed = stmt.execute(params![
                    event.ts.format("%Y-%m-%dT%H:%M:%S").to_string(),
                    event.kind.as_str(),
                    serde_json::to_string(&event.payload)?,
                    file_offset,
                    batch.generation,
                ])?;
                added += changed as u32;
            }
        }

        for update in &batch.zones {
            tx.execute(
                "INSERT INTO zones (code, name, act, area_level) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(code) DO UPDATE SET
                    name = COALESCE(excluded.name, zones.name),
                    act = COALESCE(excluded.act, zones.act),
                    area_level = excluded.area_level",
                params![update.code, update.name, update.act, update.area_level],
            )?;
        }

        for (name, ascendancy, ts) in &batch.characters {
            tx.execute(
                "INSERT INTO characters (name, ascendancy, last_seen_ts) VALUES (?1, ?2, ?3)
                 ON CONFLICT(name) DO UPDATE SET
                    ascendancy = COALESCE(excluded.ascendancy, characters.ascendancy),
                    last_seen_ts = COALESCE(excluded.last_seen_ts, characters.last_seen_ts)",
                params![
                    name,
                    ascendancy,
                    ts.map(|t| t.format("%Y-%m-%dT%H:%M:%S").to_string()),
                ],
            )?;
        }

        for (key, value) in &batch.meta {
            tx.execute(
                "INSERT INTO meta (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )?;
        }

        tx.commit()?;
        Ok(added)
    }

    /// Empties the derived tables. The event log is never touched — it is the
    /// source of truth these tables are rebuilt from.
    pub fn clear_derived(&mut self) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM zones", [])?;
        tx.execute("DELETE FROM characters", [])?;
        tx.commit()?;
        Ok(())
    }
```

Add the batch type and the two row types above `impl Poe2Store`:

```rust
/// Everything one poll of the log produced, written together or not at all.
pub struct IngestBatch {
    pub events: Vec<(Event, i64)>,
    pub generation: i64,
    pub zones: Vec<ZoneUpdate>,
    pub characters: Vec<(String, Option<String>, Option<NaiveDateTime>)>,
    pub meta: Vec<(String, String)>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ZoneRow {
    pub code: String,
    pub name: Option<String>,
    pub act: Option<String>,
    pub area_level: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct CharacterRow {
    pub name: String,
    pub ascendancy: Option<String>,
    pub last_seen_ts: Option<String>,
}
```

Add to the file's imports: `use chrono::NaiveDateTime;`, `use crate::log::events::{Event, EventKind};`, `use crate::log::zones::ZoneUpdate;`.

- [ ] **Step 5: Run the tests to verify they pass**

```bash
cd src-tauri/poe2-core && cargo test
```

Expected: PASS — 7 new tests on top of the previous total, and every earlier item test still green.

- [ ] **Step 6: Format and commit**

```bash
cd src-tauri/poe2-core && cargo fmt
cd ../.. && git add src-tauri/poe2-core
git commit -m "feat(poe2): event, zone and character tables"
```

---

## Task 6: Ingestion, the polling thread and commands

**Files:**
- Create: `src-tauri/src/poe2/tracker.rs`
- Modify: `src-tauri/src/poe2/mod.rs`, `src-tauri/src/poe2/commands.rs`, `src-tauri/src/settings.rs`, `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: everything from Tasks 1–5; `store_for` from the existing `commands.rs`.
- Produces:
  - setting `poe2_log_path: Option<String>` defaulting to `None`
  - `change_poe2_log_path_setting(app: AppHandle, path: Option<String>) -> Result<(), String>`
  - `tracker::DEFAULT_LOG_PATH: &str`
  - `tracker::log_path(app: &AppHandle) -> PathBuf`
  - `tracker::poll_once(app: &AppHandle) -> Result<u32, String>` — events added
  - `tracker::spawn(app: AppHandle)` — guarded by its own `AtomicBool`
  - `tracker::has_debug_lines(path: &Path) -> bool`
  - `poe2_state(app: AppHandle) -> Result<ProgressSnapshot, String>` where `ProgressSnapshot { state: TrackerState, zone_name: Option<String>, level_gap: Option<i64>, seconds_in_zone: Option<i64>, act: Option<String>, log_present: bool, debug_lines: bool, importing: bool, event_count: i64 }`
  - `poe2_rebuild_derived(app: AppHandle) -> Result<u32, String>` — events replayed
  - event `poe2://state-changed`, no payload

- [ ] **Step 1: Add the setting**

In `src-tauri/src/settings.rs`, inside `AppSettings`:

```rust
    #[serde(default)]
    pub poe2_log_path: Option<String>,
```

and in the `get_default_settings()` struct literal:

```rust
        poe2_log_path: None,
```

`None` rather than the Steam path as a literal: a player who moved the game or runs another platform should not be silently pinned to a path that was only ever a guess. `tracker::log_path` resolves `None` to the default at read time.

- [ ] **Step 2: Write the tracker**

Create `src-tauri/src/poe2/tracker.rs`:

```rust
//! Ingesting Client.txt into the event log, and the thread that keeps it current.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tauri::{AppHandle, Emitter};

use poe2_core::log::events::EventKind;
use poe2_core::log::parser::parse_line;
use poe2_core::log::tail::LogTail;
use poe2_core::log::zones::ZoneResolver;
use poe2_core::store::IngestBatch;

use crate::poe2::commands::store_for;
use crate::settings;

pub const DEFAULT_LOG_PATH: &str =
    r"C:\Program Files (x86)\Steam\steamapps\common\Path of Exile 2\logs\Client.txt";

pub const POLL_INTERVAL: Duration = Duration::from_secs(1);
pub const STATE_CHANGED_EVENT: &str = "poe2://state-changed";

/// Set while the first, whole-file import is running, so the interface can say
/// so — a minute of blank fields is indistinguishable from a broken feature.
pub static IMPORTING: AtomicBool = AtomicBool::new(false);

static TRACKER_RUNNING: AtomicBool = AtomicBool::new(false);

pub fn log_path(app: &AppHandle) -> PathBuf {
    match settings::get_settings(app).poe2_log_path {
        Some(path) if !path.trim().is_empty() => PathBuf::from(path),
        _ => PathBuf::from(DEFAULT_LOG_PATH),
    }
}

/// Whether the log carries `[DEBUG]` lines at all.
///
/// The game omits them unless the player enables them, and without them zone
/// changes are invisible. Only the tail of the file is sampled: reading 27 MB to
/// answer a yes/no question would stall every poll.
pub fn has_debug_lines(path: &Path) -> bool {
    use std::io::{Read, Seek, SeekFrom};
    const SAMPLE: u64 = 2_000_000;

    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let start = metadata.len().saturating_sub(SAMPLE);
    if file.seek(SeekFrom::Start(start)).is_err() {
        return false;
    }
    let mut buf = Vec::new();
    if file.take(SAMPLE).read_to_end(&mut buf).is_err() {
        return false;
    }
    String::from_utf8_lossy(&buf).contains("[DEBUG Client")
}

/// Reads whatever is new and stores it. Returns how many events were added.
pub fn poll_once(app: &AppHandle) -> Result<u32, String> {
    let path = log_path(app);
    let mut store = store_for(app)?;

    let offset: u64 = store
        .get_meta("log_offset")
        .map_err(|e| e.to_string())?
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let mut generation: i64 = store
        .get_meta("log_generation")
        .map_err(|e| e.to_string())?
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    // An empty string means "no fingerprint", which is not the same as "the
    // fingerprint of the previous file" — see LogTail for why that distinction
    // matters after a restart.
    let fingerprint = store
        .get_meta("log_fingerprint")
        .map_err(|e| e.to_string())?
        .filter(|f| !f.is_empty());

    let mut tail = LogTail::new(&path, offset, fingerprint);
    let lines = tail.read_new();
    if tail.rotated {
        generation += 1;
    }
    if lines.is_empty() {
        return Ok(0);
    }

    // The whole poll is assembled first and written in one transaction. Writing
    // per event turned the cold import of a 27 MB log into ninety seconds of
    // waiting on the disk, and it let the read offset advance past events that
    // had not been stored.
    let mut zones = ZoneResolver::new();
    let mut batch = IngestBatch {
        events: Vec::new(),
        generation,
        zones: Vec::new(),
        characters: Vec::new(),
        meta: Vec::new(),
    };

    for (line_offset, line) in &lines {
        let Some(event) = parse_line(line) else {
            continue;
        };
        batch.zones.extend(zones.feed(&event));
        match event.kind {
            EventKind::LevelUp => {
                if let Some(name) = event.payload["character"].as_str() {
                    batch.characters.push((
                        name.to_string(),
                        event.payload["ascendancy"].as_str().map(str::to_string),
                        Some(event.ts),
                    ));
                }
            }
            EventKind::QuestReward => {
                if let Some(name) = event.payload["character"].as_str() {
                    batch
                        .characters
                        .push((name.to_string(), None, Some(event.ts)));
                }
            }
            _ => {}
        }
        batch.events.push((event, *line_offset as i64));
    }

    batch.meta.push(("log_offset".into(), tail.offset.to_string()));
    batch
        .meta
        .push(("log_generation".into(), generation.to_string()));
    // An empty string means "no fingerprint" and is stored deliberately: leaving
    // the previous file's fingerprint in place would make a later, longer file
    // look like another rotation after a restart.
    batch.meta.push((
        "log_fingerprint".into(),
        tail.fingerprint.clone().unwrap_or_default(),
    ));

    store.ingest_batch(&batch).map_err(|e| e.to_string())
}

/// Starts the polling thread if the section is enabled. Safe to call repeatedly.
///
/// Like the clipboard watcher, the thread persists for the process and simply
/// stops reading while the section is off: a thread that exits on a setting
/// change races with a re-enable and can leave nothing running at all.
pub fn spawn(app: AppHandle) {
    if !settings::get_settings(&app).poe2_enabled {
        return;
    }
    if TRACKER_RUNNING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }

    std::thread::spawn(move || {
        // The first poll has the whole file to get through.
        IMPORTING.store(true, Ordering::SeqCst);
        let first = poll_once(&app);
        IMPORTING.store(false, Ordering::SeqCst);
        match first {
            Ok(n) if n > 0 => {
                let _ = app.emit(STATE_CHANGED_EVENT, ());
            }
            Err(e) => log::warn!("poe2 tracker: initial import failed: {e}"),
            _ => {}
        }

        loop {
            std::thread::sleep(POLL_INTERVAL);
            if !settings::get_settings(&app).poe2_enabled {
                continue;
            }
            match poll_once(&app) {
                Ok(0) => {}
                Ok(_) => {
                    let _ = app.emit(STATE_CHANGED_EVENT, ());
                }
                Err(e) => log::warn!("poe2 tracker: poll failed: {e}"),
            }
        }
    });
}
```

Add `pub mod tracker;` to `src-tauri/src/poe2/mod.rs`.

- [ ] **Step 3: Add the commands**

In `src-tauri/src/poe2/commands.rs`, add these imports:

```rust
use poe2_core::log::state::{build_state, level_gap, TrackerState};
use poe2_core::store::ZoneRow;
```

and these commands:

```rust
/// What the Progress tab needs, flattened.
///
/// `TrackerState` itself does not cross this boundary: its timestamps are
/// `NaiveDateTime`, which specta cannot describe without its chrono feature. The
/// two timestamps the interface actually shows are rendered as ISO strings here.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct ProgressSnapshot {
    pub character: Option<String>,
    pub ascendancy: Option<String>,
    pub level: Option<i64>,
    pub zone_code: Option<String>,
    pub zone_name: Option<String>,
    pub zone_level: Option<i64>,
    pub character_confirmed_ts: Option<String>,
    pub last_ts: Option<String>,
    pub focused: bool,
    pub rewards: Vec<String>,
    pub level_gap: Option<i64>,
    pub seconds_in_zone: Option<i64>,
    /// The current zone's act, which is more trustworthy than the last act seen:
    /// a global "last act" never resets and would show an act finished hours ago
    /// once the player reaches the endgame or a hideout.
    pub act: Option<String>,
    pub log_present: bool,
    pub debug_lines: bool,
    pub importing: bool,
    pub event_count: i64,
}

#[tauri::command]
#[specta::specta]
pub fn poe2_state(app: AppHandle) -> Result<ProgressSnapshot, String> {
    let store = store_for(&app)?;
    let events = store.events().map_err(|e| e.to_string())?;
    let state = build_state(&events);

    let zones: Vec<ZoneRow> = store.zones().map_err(|e| e.to_string())?;
    let zone = zones
        .iter()
        .find(|z| Some(z.code.as_str()) == state.zone_code.as_deref());

    let seconds_in_zone = state
        .zone_since
        .map(|since| (chrono::Local::now().naive_local() - since).num_seconds());

    let rewards = state
        .character
        .as_ref()
        .and_then(|name| state.rewards.get(name))
        .cloned()
        .unwrap_or_default();

    let path = crate::poe2::tracker::log_path(&app);
    Ok(ProgressSnapshot {
        character: state.character.clone(),
        ascendancy: state.ascendancy.clone(),
        level: state.level,
        zone_code: state.zone_code.clone(),
        zone_name: zone.and_then(|z| z.name.clone()),
        zone_level: state.zone_level,
        character_confirmed_ts: state.character_confirmed_ts.map(|t| t.to_string()),
        last_ts: state.last_ts.map(|t| t.to_string()),
        focused: state.focused,
        rewards,
        level_gap: level_gap(&state),
        seconds_in_zone,
        act: zone.and_then(|z| z.act.clone()).or_else(|| state.act.clone()),
        log_present: path.exists(),
        debug_lines: crate::poe2::tracker::has_debug_lines(&path),
        importing: crate::poe2::tracker::IMPORTING.load(std::sync::atomic::Ordering::SeqCst),
        event_count: events.len() as i64,
    })
}

/// Replays the stored events to rebuild `zones` and `characters`.
///
/// The event log itself is never touched. This exists so a fix to the zone
/// pairing rules can be applied to history that is already ingested.
#[tauri::command]
#[specta::specta]
pub fn poe2_rebuild_derived(app: AppHandle) -> Result<u32, String> {
    let mut store = store_for(&app)?;
    let events = store.events().map_err(|e| e.to_string())?;
    store.clear_derived().map_err(|e| e.to_string())?;

    let mut zones = poe2_core::log::zones::ZoneResolver::new();
    for event in &events {
        for update in zones.feed(event) {
            store.apply_zone_update(&update).map_err(|e| e.to_string())?;
        }
        match event.kind {
            poe2_core::log::events::EventKind::LevelUp => {
                if let Some(name) = event.payload["character"].as_str() {
                    store
                        .upsert_character(name, event.payload["ascendancy"].as_str(), Some(event.ts))
                        .map_err(|e| e.to_string())?;
                }
            }
            poe2_core::log::events::EventKind::QuestReward => {
                if let Some(name) = event.payload["character"].as_str() {
                    store
                        .upsert_character(name, None, Some(event.ts))
                        .map_err(|e| e.to_string())?;
                }
            }
            _ => {}
        }
    }
    Ok(events.len() as u32)
}

#[tauri::command]
#[specta::specta]
pub fn change_poe2_log_path_setting(app: AppHandle, path: Option<String>) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.poe2_log_path = path;
    settings::write_settings(&app, settings.clone());
    Ok(())
}
```

In the existing `change_poe2_enabled_setting`, alongside the `watcher::spawn(app)` call already there, also start the tracker:

```rust
    if enabled {
        crate::poe2::watcher::spawn(app.clone());
        crate::poe2::tracker::spawn(app);
    }
```

- [ ] **Step 4: Register everything**

In `src-tauri/src/lib.rs`, add to `collect_commands![…]`:

```rust
            poe2::commands::poe2_state,
            poe2::commands::poe2_rebuild_derived,
            poe2::commands::change_poe2_log_path_setting,
```

and in the `.setup(...)` closure, beside the existing watcher spawn:

```rust
            crate::poe2::tracker::spawn(app.handle().clone());
```

- [ ] **Step 5: Build**

```bash
cd src-tauri && cargo build
```

Expected: compiles. Takes a few minutes.

- [ ] **Step 6: Verify the core tests still pass**

```bash
cd src-tauri/poe2-core && cargo test
```

Expected: PASS, unchanged count — this task adds no `poe2-core` tests.

- [ ] **Step 7: Format and commit**

```bash
cd src-tauri && cargo fmt
cd .. && git status --short
```

Revert any file the formatter touched that you did not edit, then:

```bash
git add src-tauri/src
git commit -m "feat(poe2): ingest the game log and serve tracker state"
```

---

## Task 7: The Progress tab

**Files:**
- Create: `src/components/poe2/Poe2Page.tsx`, `src/components/poe2/ProgressTab.tsx`
- Modify: `src/components/Sidebar.tsx`, `src/stores/settingsStore.ts`, `src/i18n/locales/en/translation.json`, `src/i18n/locales/ru/translation.json`

**Interfaces:**
- Consumes: `commands.poe2State`, `commands.poe2RebuildDerived`, `commands.changePoe2LogPathSetting` from the generated `src/bindings.ts`; the `poe2://state-changed` event.
- Produces: no interfaces for other tasks.

- [ ] **Step 1: Add the i18n keys**

In `src/i18n/locales/en/translation.json`, add to the existing `"poe2"` object, beside `"items"`:

```json
    "tabs": {
      "progress": "Progress",
      "items": "Items"
    },
    "progress": {
      "noCharacter": "No character identified yet",
      "confirmed": "character confirmed {{when}}",
      "unconfirmed": "no event has named a character yet",
      "level": "Level",
      "act": "Act",
      "zoneLevel": "Zone level",
      "inZone": "In zone",
      "zone": "Current zone",
      "gap": "Character level against zone level",
      "gapUnknown": "needs both a character level and a zone level",
      "gapBehind": "noticeably below the zone — worth levelling up",
      "gapSlightly": "slightly behind",
      "gapFine": "level is fine",
      "rewards": "Quest rewards",
      "noRewards": "no rewards yet",
      "importing": "Reading the game log for the first time — this takes a moment.",
      "noLog": "Game log not found at {{path}}",
      "noDebug": "The game log has no DEBUG lines, so zone changes are not tracked.",
      "logPath": "Game log",
      "choose": "Choose file…",
      "reset": "Use the default path",
      "rebuild": "Rebuild zones and characters",
      "rebuilt": "Replayed {{count}} events.",
      "rebuildError": "Could not rebuild from the event log.",
      "events": "{{count}} events stored"
    }
```

The same keys in `src/i18n/locales/ru/translation.json`:

```json
    "tabs": {
      "progress": "Прогресс",
      "items": "Предметы"
    },
    "progress": {
      "noCharacter": "Персонаж пока не определён",
      "confirmed": "персонаж подтверждён {{when}}",
      "unconfirmed": "ни одно событие ещё не назвало персонажа",
      "level": "Уровень",
      "act": "Акт",
      "zoneLevel": "Уровень зоны",
      "inZone": "В зоне",
      "zone": "Текущая зона",
      "gap": "Уровень персонажа против уровня зоны",
      "gapUnknown": "нужен уровень персонажа и уровень зоны",
      "gapBehind": "персонаж заметно ниже зоны — стоит подкачаться",
      "gapSlightly": "небольшое отставание",
      "gapFine": "уровень в порядке",
      "rewards": "Квестовые награды",
      "noRewards": "наград пока нет",
      "importing": "Первое чтение журнала игры — это займёт несколько секунд.",
      "noLog": "Журнал игры не найден: {{path}}",
      "noDebug": "В журнале игры нет строк DEBUG, поэтому смена зон не отслеживается.",
      "logPath": "Журнал игры",
      "choose": "Выбрать файл…",
      "reset": "Вернуть путь по умолчанию",
      "rebuild": "Пересобрать зоны и персонажей",
      "rebuilt": "Просмотрено событий: {{count}}.",
      "rebuildError": "Не удалось пересобрать из журнала событий.",
      "events": "событий в базе: {{count}}"
    }
```

- [ ] **Step 2: Write the Progress tab**

Create `src/components/poe2/ProgressTab.tsx`:

```tsx
import React, { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { commands, type ProgressSnapshot } from "../../bindings";
import { Button } from "../ui/Button";
import { useSettings } from "../../hooks/useSettings";

// Emitted by src-tauri/src/poe2/tracker.rs after a poll that added events.
const STATE_CHANGED_EVENT = "poe2://state-changed";

function formatDuration(seconds: number | null): string {
  if (seconds === null) return "—";
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes} min ${seconds % 60} s`;
  return `${Math.floor(minutes / 60)} h ${minutes % 60} min`;
}

export const ProgressTab: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting } = useSettings();
  const [snapshot, setSnapshot] = useState<ProgressSnapshot | null>(null);
  const [status, setStatus] = useState("");

  const load = useCallback(async () => {
    const result = await commands.poe2State();
    if (result.status === "ok") setSnapshot(result.data);
  }, []);

  useEffect(() => {
    void load();
    const unlisten = listen(STATE_CHANGED_EVENT, () => {
      void load();
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, [load]);

  const chooseLogPath = useCallback(async () => {
    const picked = await open({ multiple: false, directory: false });
    if (typeof picked === "string") {
      await updateSetting("poe2_log_path", picked);
      await load();
    }
  }, [updateSetting, load]);

  const resetLogPath = useCallback(async () => {
    await updateSetting("poe2_log_path", null);
    await load();
  }, [updateSetting, load]);

  const rebuild = useCallback(async () => {
    const result = await commands.poe2RebuildDerived();
    setStatus(
      result.status === "ok"
        ? t("poe2.progress.rebuilt", { count: result.data })
        : t("poe2.progress.rebuildError"),
    );
    await load();
  }, [load, t]);

  if (!snapshot) return null;

  const s = snapshot;
  const gap = snapshot.level_gap;
  const gapNote =
    gap === null
      ? t("poe2.progress.gapUnknown")
      : gap < -2
        ? t("poe2.progress.gapBehind")
        : gap >= 0
          ? t("poe2.progress.gapFine")
          : t("poe2.progress.gapSlightly");

  // Already narrowed to the active character by the backend.
  const rewards = snapshot.rewards;
  const logPath = (getSetting("poe2_log_path") as string | null) ?? null;

  return (
    <div className="p-4 space-y-3">
      {snapshot.importing && (
        <p className="text-sm opacity-70">{t("poe2.progress.importing")}</p>
      )}
      {!snapshot.log_present && (
        <p className="text-sm opacity-70">
          {t("poe2.progress.noLog", { path: logPath ?? "" })}
        </p>
      )}
      {snapshot.log_present && !snapshot.debug_lines && (
        <p className="text-sm opacity-70">{t("poe2.progress.noDebug")}</p>
      )}

      <div>
        <h2 className="text-lg font-semibold">
          {s.character ?? t("poe2.progress.noCharacter")}
        </h2>
        <p className="text-sm opacity-60">{s.ascendancy ?? ""}</p>
        <p className="text-sm opacity-60">
          {s.character_confirmed_ts
            ? t("poe2.progress.confirmed", { when: s.character_confirmed_ts })
            : t("poe2.progress.unconfirmed")}
        </p>
      </div>

      <div className="rounded-md border border-mid-gray/30 p-3">
        <p className="text-sm opacity-60">{t("poe2.progress.gap")}</p>
        <p className="text-2xl font-semibold">
          {gap === null ? "—" : gap > 0 ? `+${gap}` : gap}
        </p>
        <p className="text-sm opacity-60">{gapNote}</p>
      </div>

      <div className="grid grid-cols-2 gap-2">
        <div className="rounded-md border border-mid-gray/30 p-3">
          <p className="text-sm opacity-60">{t("poe2.progress.level")}</p>
          <p className="text-xl">{s.level ?? "—"}</p>
        </div>
        <div className="rounded-md border border-mid-gray/30 p-3">
          <p className="text-sm opacity-60">{t("poe2.progress.act")}</p>
          <p className="text-xl">{snapshot.act ?? "—"}</p>
        </div>
        <div className="rounded-md border border-mid-gray/30 p-3">
          <p className="text-sm opacity-60">{t("poe2.progress.zoneLevel")}</p>
          <p className="text-xl">{s.zone_level ?? "—"}</p>
        </div>
        <div className="rounded-md border border-mid-gray/30 p-3">
          <p className="text-sm opacity-60">{t("poe2.progress.inZone")}</p>
          <p className="text-xl">{formatDuration(snapshot.seconds_in_zone)}</p>
        </div>
      </div>

      <div className="rounded-md border border-mid-gray/30 p-3">
        <p className="text-sm opacity-60">{t("poe2.progress.zone")}</p>
        <p className="text-xl">{snapshot.zone_name ?? s.zone_code ?? "—"}</p>
        {snapshot.zone_name && <p className="text-sm opacity-60">{s.zone_code}</p>}
      </div>

      <div className="rounded-md border border-mid-gray/30 p-3">
        <p className="text-sm opacity-60">{t("poe2.progress.rewards")}</p>
        {rewards.length === 0 ? (
          <p className="text-sm opacity-60">{t("poe2.progress.noRewards")}</p>
        ) : (
          <ul className="list-disc pl-5 text-sm">
            {rewards.map((r, i) => (
              <li key={`${r}-${i}`}>{r}</li>
            ))}
          </ul>
        )}
      </div>

      <div className="space-y-2">
        <p className="text-sm opacity-60">{t("poe2.progress.logPath")}</p>
        <p className="text-sm break-all opacity-80">{logPath ?? ""}</p>
        <div className="flex flex-wrap items-center gap-2">
          <Button onClick={chooseLogPath}>{t("poe2.progress.choose")}</Button>
          <Button onClick={resetLogPath}>{t("poe2.progress.reset")}</Button>
          <Button onClick={rebuild}>{t("poe2.progress.rebuild")}</Button>
        </div>
        <p className="text-sm opacity-60">
          {t("poe2.progress.events", { count: snapshot.event_count })} {status}
        </p>
      </div>
    </div>
  );
};
```

Rewards come from the game log, which sits beside other players' chat, so every one of them is rendered as a React text child and never as markup.

- [ ] **Step 3: Write the tab container**

Create `src/components/poe2/Poe2Page.tsx`:

```tsx
import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { ItemsPage } from "./ItemsPage";
import { ProgressTab } from "./ProgressTab";

type Tab = "progress" | "items";

export const Poe2Page: React.FC = () => {
  const { t } = useTranslation();
  const [tab, setTab] = useState<Tab>("progress");

  return (
    <div className="max-w-3xl w-full mx-auto">
      <div className="flex gap-2 border-b border-mid-gray/30 px-4 pt-3">
        {(["progress", "items"] as Tab[]).map((id) => (
          <button
            key={id}
            type="button"
            onClick={() => setTab(id)}
            className={
              tab === id
                ? "border-b-2 border-current px-3 py-2 font-medium"
                : "px-3 py-2 opacity-60"
            }
          >
            {t(`poe2.tabs.${id}`)}
          </button>
        ))}
      </div>
      {tab === "progress" ? <ProgressTab /> : <ItemsPage />}
    </div>
  );
};
```

- [ ] **Step 4: Point the sidebar at the new page**

In `src/components/Sidebar.tsx`, change the import of `ItemsPage` to `Poe2Page`:

```tsx
import { Poe2Page } from "./poe2/Poe2Page";
```

and in `SECTIONS_CONFIG`, change the `poe2` entry's `component` from `ItemsPage` to `Poe2Page`. Leave its `labelKey`, `icon` and `enabled` untouched.

- [ ] **Step 5: Register the settings updater**

In `src/stores/settingsStore.ts`, add to the `settingUpdaters` map:

```ts
  poe2_log_path: (value) =>
    commands.changePoe2LogPathSetting(value as string | null),
```

Without this entry the setting silently fails to persist and only logs to the console.

- [ ] **Step 6: Verify**

```bash
bun run lint
bunx tsc --noEmit
```

Both must be clean. If `commands.poe2State` or the `ProgressSnapshot` type is missing from `src/bindings.ts`, run `bun run tauri dev` once to regenerate it, then stop it — never hand-edit that file.

- [ ] **Step 7: Commit**

```bash
git add src/components/poe2 src/components/Sidebar.tsx src/stores/settingsStore.ts src/i18n
git commit -m "feat(poe2): progress tab beside the items tab"
```

---

## Task 8: Acceptance against the definition of done

**Files:**
- Create: `src-tauri/poe2-core/src/log/acceptance.rs`
- Modify: `src-tauri/poe2-core/src/log/mod.rs`

**Interfaces:**
- Consumes: everything from Tasks 1–5.
- Produces: nothing new.

Two of the spec's criteria cannot be tested in this crate — they live in Tauri-side code and the UI. Do not fake them; list them in a module doc comment naming where each is enforced, exactly as `src/acceptance.rs` already does for the item criteria.

- [ ] **Step 1: Write the acceptance tests**

Create `src-tauri/poe2-core/src/log/acceptance.rs`:

```rust
//! Acceptance tests for the definition of done in
//! docs/superpowers/specs/2026-08-05-poe2-log-tracker-design.md.
//!
//! Two criteria cannot be expressed here and are enforced elsewhere:
//!   - Criterion 2's "updates within about a second" and criterion 4's
//!     "does not block the window" live in the polling thread in
//!     src-tauri/src/poe2/tracker.rs.
//!   - Criterion 8, i18n labels, lives in src/i18n/locales/{en,ru}/translation.json
//!     and src/components/poe2/ProgressTab.tsx.

#[cfg(test)]
mod tests {
    use crate::log::events::EventKind;
    use crate::log::parser::parse_line;
    use crate::log::state::{build_state, level_gap};
    use crate::log::tail::{LogTail, FINGERPRINT_BYTES};
    use crate::log::zones::ZoneResolver;
    use crate::store::Poe2Store;
    use chrono::NaiveDate;
    use serde_json::json;

    const SAMPLE: &str = include_str!("fixtures/sample_client.txt");

    /// Criterion 5: a log with no DEBUG lines must be detectable, because
    /// without them zone changes are invisible and the interface has to say so.
    #[test]
    fn debug_lines_are_distinguishable_in_the_sample() {
        let debug_lines = SAMPLE
            .lines()
            .filter(|l| l.contains("[DEBUG Client"))
            .count();
        assert_eq!(debug_lines, 1, "the fixture carries exactly one DEBUG line");
    }

    /// Criterion 6: a zone entry followed by both kinds of Set Source records
    /// both the name and the act.
    #[test]
    fn a_zone_gets_both_its_name_and_its_act() {
        let base = NaiveDate::from_ymd_opt(2026, 8, 2).unwrap();
        let mut resolver = ZoneResolver::new();
        let area = crate::log::events::Event {
            ts: base.and_hms_opt(12, 0, 0).unwrap(),
            kind: EventKind::AreaEntered,
            payload: json!({"code": "G1_4", "area_level": 4}),
        };
        let named = crate::log::events::Event {
            ts: base.and_hms_opt(12, 0, 1).unwrap(),
            kind: EventKind::Scene,
            payload: json!({"source": "Clearfell Encampment"}),
        };
        let acted = crate::log::events::Event {
            ts: base.and_hms_opt(12, 0, 2).unwrap(),
            kind: EventKind::Scene,
            payload: json!({"source": "Act 1"}),
        };

        let mut store = Poe2Store::in_memory().unwrap();
        for event in [&area, &named, &acted] {
            for update in resolver.feed(event) {
                store.apply_zone_update(&update).unwrap();
            }
        }

        let zones = store.zones().unwrap();
        assert_eq!(zones.len(), 1);
        assert_eq!(zones[0].name.as_deref(), Some("Clearfell Encampment"));
        assert_eq!(zones[0].act.as_deref(), Some("Act 1"));
    }

    /// Criterion 3: a rotated log starts a new generation and loses no events.
    #[test]
    fn rotation_starts_a_generation_and_keeps_everything() {
        let mut store = Poe2Store::in_memory().unwrap();
        let event = crate::log::events::Event {
            ts: NaiveDate::from_ymd_opt(2026, 8, 2).unwrap().and_hms_opt(12, 0, 0).unwrap(),
            kind: EventKind::Focus,
            payload: json!({"gained": true}),
        };
        assert!(store.add_event(&event, 42, 0).unwrap());
        // After rotation the same byte offset comes round again.
        assert!(store.add_event(&event, 42, 1).unwrap());
        assert_eq!(store.events().unwrap().len(), 2);
    }

    /// Criterion 7: rebuilding derived tables never touches the event log.
    #[test]
    fn rebuilding_derived_tables_leaves_events_untouched() {
        let mut store = Poe2Store::in_memory().unwrap();
        let event = crate::log::events::Event {
            ts: NaiveDate::from_ymd_opt(2026, 8, 2).unwrap().and_hms_opt(12, 0, 0).unwrap(),
            kind: EventKind::LevelUp,
            payload: json!({"character": "Hero", "ascendancy": "Sorceress", "level": 5}),
        };
        store.add_event(&event, 10, 0).unwrap();
        store.upsert_character("Hero", Some("Sorceress"), Some(event.ts)).unwrap();

        store.clear_derived().unwrap();

        assert!(store.characters().unwrap().is_empty());
        assert_eq!(store.events().unwrap().len(), 1);
    }

    /// The whole pipeline on the real fixture: lines in, state out.
    #[test]
    fn the_fixture_produces_a_coherent_state() {
        let events: Vec<_> = SAMPLE.lines().filter_map(parse_line).collect();
        let state = build_state(&events);
        assert_eq!(state.character.as_deref(), Some("Kasablankee"));
        assert_eq!(state.zone_code.as_deref(), Some("P2_2"));
        assert_eq!(state.zone_level, Some(59));
        // The level came from a line earlier than the zone, and the character
        // never changed, so it stands.
        assert_eq!(state.level, Some(61));
        assert_eq!(level_gap(&state), Some(2));
        let rewards = state.rewards.get("Kasablankee").unwrap();
        assert!(rewards.contains(&"+10% to Cold Resistance".to_string()));
        assert!(rewards.contains(&"+1 Charm Slot".to_string()));
        // The chat line that reads like a reward must not be among them.
        assert!(!rewards.iter().any(|r| r.contains("Spirit")));
    }

    /// A file replaced by one of the same length is caught only by the
    /// fingerprint — size alone cannot see it.
    #[test]
    fn same_length_replacement_is_detected() {
        let mut path = std::env::temp_dir();
        path.push(format!("poe2-acceptance-{}.txt", std::process::id()));
        let _ = std::fs::remove_file(&path);

        std::fs::write(&path, "A".repeat(FINGERPRINT_BYTES) + "\n").unwrap();
        let mut tail = LogTail::new(&path, 0, None);
        tail.read_new();
        assert!(!tail.rotated);

        std::fs::write(&path, "B".repeat(FINGERPRINT_BYTES) + "\n").unwrap();
        tail.read_new();
        assert!(tail.rotated);
        std::fs::remove_file(&path).ok();
    }
}
```

Add to `src-tauri/poe2-core/src/log/mod.rs`:

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

All must be clean. `cd src-tauri && cargo fmt -- --check` fails on this fork's pre-existing drift in `actions.rs` and `tray.rs` — report that as pre-existing rather than fixing unrelated files.

- [ ] **Step 4: Verify against the real log**

```bash
cd src-tauri && cargo build && cd .. && bun run tauri dev
```

Enable the Path of Exile 2 section, open the Progress tab, and let the first import finish. Expected against the player's real `Client.txt`: roughly 14,474 events, 140 zones, 19 characters, one generation. If the counts are far off, stop and report — the parser has drifted from the Python original.

If the machine's screen is locked or the app cannot be driven, say so plainly rather than reporting this step as done.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/poe2-core
git commit -m "test(poe2): acceptance tests for the log tracker"
```

---

## Checking the plan against the spec's definition of done

| Criterion from the spec | Where it is met |
|---|---|
| 1. Real log reproduces ~14,474 events, 140 zones, 19 characters, one generation | Task 8 step 4 |
| 2. Progress tab shows the full state and updates within about a second | Tasks 6–7; the poll interval in `tracker.rs` |
| 3. A truncated or replaced log starts a new generation, losing nothing | Task 2, `truncation_is_detected_and_restarts_from_zero`; Task 5, `the_same_offset_in_a_new_generation_is_kept`; Task 8, `rotation_starts_a_generation_and_keeps_everything` |
| 4. The 27 MB import does not block the window, and the tab says so | Task 6, `spawn` and `IMPORTING`; Task 7, the importing banner |
| 5. A log without `[DEBUG]` lines produces a visible warning | Task 6, `has_debug_lines`; Task 7, the noDebug banner; Task 8, `debug_lines_are_distinguishable_in_the_sample` |
| 6. Both zone name and act are recorded | Task 4, `both_the_name_and_the_act_are_recorded`; Task 8, `a_zone_gets_both_its_name_and_its_act` |
| 7. The event log is never mutated | Task 5, `clear_derived_leaves_the_event_log_alone`; Task 8, `rebuilding_derived_tables_leaves_events_untouched` |
| 8. Labels come from i18n in English and Russian | Task 7 step 1 |
| Chat must never be recorded as a quest reward | Task 1, `chat_message_is_not_a_quest_reward`; Task 8, the fixture state check |
| Act comes from the current zone, not the last act seen | Task 6, `ProgressSnapshot.act` |
| One transaction per poll | Task 5, `ingest_batch` and `a_batch_lands_in_one_transaction`; Task 6, `poll_once` assembles the whole poll before writing it. `add_event` remains for single-event use in tests |
| UTF-8 read with replacement | Task 2, `invalid_utf8_is_replaced_not_fatal` |
| Game files opened read-only | Task 2 — `tail.rs` only ever calls `File::open` |
