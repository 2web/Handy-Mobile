# Path of Exile 2 Log Tracker — Design

**Date:** 2026-08-05
**Status:** Approved (design), pending spec review

## Goal

Read the game's `Client.txt`, turn it into an immutable event log, and derive the player's
current state from it — character, level, zone, act, time in zone, quest rewards — shown
live in Handy while the game runs.

This is the second of five ports (R1) bringing the `poe2-helper` Python program into Handy.
The parsing rules below are not new: they were derived from seven months of one player's
real log and debugged against it. What is new is the language and the host application.

## Context

Handy already carries R3, item capture: a "Path of Exile 2" sidebar section, a `poe2-core`
crate holding logic with no Tauri dependency, a `poe2.db` database with its own migrations,
and a background clipboard watcher. R1 adds the log side of the same feature set.

The port order is R3 (items, done) → **R1 (this document)** → R2 (norms and advice) →
R4 (character panel from a screenshot) → R5 (voice).

### Nothing needs migrating

The Python program's database holds 14,474 events spanning 2025-12-30 to 2026-08-05, all in
a single generation. `Client.txt` covers exactly that period and the game never truncated
it. So the Rust version reproduces the entire history by reading the log — 27 MB, one pass.
The Python database stays untouched as a fallback and is archived when the whole port is
done, not before.

## What the log gives

Every line looks like this:

```
2026/08/02 12:05:17 9674984 3ef231e0 [DEBUG Client 23396] <message>
```

Seven message shapes matter, and each becomes an event:

| Event | Recognised from | Payload |
|---|---|---|
| `area_entered` | `Generating level 4 area "G1_4"` | `code`, `area_level` |
| `level_up` | `: Hero (Sorceress) is now level 12` | `character`, `ascendancy`, `level` |
| `quest_reward` | `: Hero has received +10% to Cold Resistance` | `character`, `reward` |
| `scene` | `[SCENE] Set Source [Act 1]` | `source` |
| `focus` | `[WINDOW] Gained focus` | `gained` |
| `disconnect` | `Abnormal disconnect:` | — |
| `slain` | `: Hero has been slain.` | `name` |

Everything else is ignored. Quest reward text carries the game's own markup, `[A|B]` and
`[A]`, which is stripped to `B` and `A`.

**Zone names are not in the zone line.** `Generating level` gives a code and a level;
the human-readable name arrives moments later in a separate `Set Source` line. The zone
directory therefore builds itself by pairing them within a time window.

## Scope & decisions (locked with user)

- **Two tabs inside the existing section:** "Progress" and "Items". No new sidebar entry —
  the game section stays one item in a dictation app's sidebar.
- **Log path is a setting**, `poe2_log_path: Option<String>`, with a file picker
  (`tauri-plugin-dialog` is already a dependency). Unset means the standard Steam location,
  `C:\Program Files (x86)\Steam\steamapps\common\Path of Exile 2\logs\Client.txt` — stored as
  `None` rather than as that literal string, so a player who moves the game or runs another
  platform is not silently pinned to a path that was only ever a guess.
- **The event log is immutable.** Derived tables — zones, characters — are rebuilt from it
  on demand, never patched in place.
- **Live updates** follow the pattern R3 established: a background thread polls once a
  second and emits a payload-less Tauri event; the tab refetches through a command.
- **Norms, advice, the character panel and voice are out of scope** — R2, R4, R5.

## Architecture

```
Client.txt ──> tail ──> parser ──> events (immutable) ──> state (fold)
                                        │                     │
                                        v                     v
                                 zones, characters      Progress tab
```

### New files in `poe2-core`

| File | Responsibility |
|---|---|
| `src/log/mod.rs` | module wiring |
| `src/log/events.rs` | `Event`, its payload enum, the seven type constants |
| `src/log/parser.rs` | line → event; **every log regex lives here and only here** |
| `src/log/tail.rs` | incremental reads, fingerprinting, rotation detection |
| `src/log/state.rs` | folding events into `TrackerState`; `level_gap` |
| `src/log/zones.rs` | pairing zone codes with the names that follow them |
| `src/log/fixtures/sample_client.txt` | a real log excerpt, embedded via `include_str!` |

`src/store.rs` gains `events`, `zones` and `characters` tables through new migrations.

### New files in the `handy` crate

| File | Responsibility |
|---|---|
| `src/poe2/tracker.rs` | the polling thread, ingestion, `poe2://state-changed` |
| `src/poe2/commands.rs` | extended with `poe2_state`, `poe2_import_log`, `poe2_rebuild_derived` |

### Frontend

`src/components/poe2/ProgressTab.tsx` shows character, ascendancy, level, act, zone name and
level, the level gap, time in zone, and the quest rewards taken. `ItemsPage.tsx` becomes the
"Items" tab beside it, its content unchanged.

## Seven details the Python version paid for in debugging

These are not implementation preferences. Each was a bug once, and re-deriving them costs
the same days a second time.

**1. The duplicate key is `(generation, file_offset)`, not the offset alone.** After the game
truncates or replaces `Client.txt`, byte offsets start over and repeat. With the offset alone
as the key, every event of the new generation is silently discarded as a duplicate.

**2. Rotation is detected by a fingerprint of the first 512 bytes, not by size.** A
replacement file that happens to be longer than the old offset looks like ordinary growth.
A file shorter than 512 bytes gets no fingerprint at all: while it is that short, appending
changes the hash, and a changing hash would read as endless rotation.

**3. One transaction per poll.** Committing per event turned the cold import of 27 MB into
ninety seconds, almost all of it waiting on the disk.

**4. Files are read as UTF-8 with invalid sequences replaced.** The log contains bytes that
are not valid UTF-8, and a strict decoder stops the whole ingest on one of them.

**5. A log with no `[DEBUG]` lines means zone changes are invisible.** The game hides those
lines unless the player enables them. The interface must say so, because otherwise the
program silently shows nothing where a zone should be, and looks broken rather than
unconfigured.

**6. The act comes from the current zone, not from the last act seen.** A global "last act"
value never resets, so in the endgame or a hideout it displays an act finished hours ago.

**7. A pending zone is not cleared by the first `Set Source` that follows it.** One zone
entry is followed by several such lines — the human name and `Act N` arrive separately, name
first. Clearing on the first match means the act is never recorded at all.

## Data model

`events` — `id`, `ts`, `type`, `payload` (JSON), `file_offset`, `generation`, with
`UNIQUE(generation, file_offset)`. Append-only; nothing ever updates or deletes a row.

`zones` — `code` (PK), `name`, `act`, `area_level`. Derived. Names are written with
`COALESCE` so a later observation cannot blank an already-known name.

`characters` — `name` (PK), `ascendancy`, `last_seen_ts`. Derived, same `COALESCE` rule.

`meta` — key/value, holding the read offset, the current generation and the file
fingerprint between runs.

The zone directory can be wrong about a *name* and self-heals on the next correct
observation. That is acceptable because nothing but the on-screen label depends on it:
zone code and level come from the event stream, never from this table.

## State

```rust
pub struct TrackerState {
    pub character: Option<String>,
    pub ascendancy: Option<String>,
    pub level: Option<i64>,
    pub zone_code: Option<String>,
    pub zone_level: Option<i64>,
    pub zone_since: Option<DateTime<Utc>>,
    pub act: Option<String>,
    pub character_confirmed_ts: Option<DateTime<Utc>>,
    pub focused: bool,
    pub last_ts: Option<DateTime<Utc>>,
    pub rewards: BTreeMap<String, Vec<String>>,
    pub known_characters: BTreeMap<String, String>,
}
```

The active character is whoever's name appeared last, which means that after switching to a
character who has not yet levelled or taken a reward, the previous one is still shown.
`character_confirmed_ts` records when the identity was last confirmed, so the interface can
show how fresh that claim is rather than asserting it flatly.

Switching character clears the level: it is unknown until that character levels up. Showing
the previous character's level against the new character's zone would produce a confident,
wrong level gap.

## Live updates and the first run

A background thread polls once a second, ingests whatever is new, and emits
`poe2://state-changed` when the poll produced at least one event. The Progress tab listens
and refetches. No payload travels with the event.

The first run has 27 MB to parse. It happens on the background thread so the window stays
responsive, and the tab shows that an initial import is in progress — a minute of blank
fields is indistinguishable from a broken feature.

The thread starts when the section is enabled, on the same `poe2_enabled` flag that gates the
sidebar entry, and it starts at app launch when that flag is already set. There is no
separate switch: reading a log file the game wrote is not the kind of act that needs its own
consent, unlike watching the clipboard. The first poll after a fresh install therefore does
the whole import, and every later poll reads only what was appended.

## Failure behaviour

| Situation | Behaviour |
|---|---|
| Log file missing | Tab says so and names the configured path; nothing else breaks |
| Log has no `[DEBUG]` lines | Warning that zone changes will not be tracked |
| File truncated or replaced | New generation, offsets restart, no events lost |
| File deleted between `stat` and `read` | The poll is skipped, not failed |
| Invalid UTF-8 in a line | Replaced, the line still parses |
| Unrecognised line | Counted and ignored |
| Character not yet confirmed | Fields shown with the time of last confirmation |

## Testing

- `parser.rs` — table-driven over real log lines: each of the seven shapes, plus lines that
  must produce nothing.
- `tail.rs` — a temp file grown, truncated and replaced; assert rotation is detected on
  truncation and on same-size replacement with different content, and that a partial trailing
  line is not returned until its newline arrives.
- `state.rs` — folding fixtures: character switch clears the level, act comes from `scene`,
  rewards accumulate per character, `level_gap` is `None` when either side is unknown.
- `zones.rs` — a zone entry followed by two `Set Source` lines records both name and act;
  a `Set Source` arriving after the window is ignored.
- `store.rs` — the same event twice at one offset is stored once; the same offset in a new
  generation is stored.
- Acceptance runs against the player's real `Client.txt` when present, and is skipped when
  it is not — it must never be the reason the suite fails on another machine.

## Definition of done

1. Reading the real `Client.txt` reproduces the Python program's counts: ~14,474 events,
   140 zones, 19 characters, in one generation.
2. The Progress tab shows character, ascendancy, level, act, zone name and level, level gap,
   time in zone, and rewards, and updates within about a second of the game writing a line.
3. A truncated or replaced log starts a new generation and loses no stored events.
4. Ingesting the full 27 MB does not block the window, and the tab says an import is running.
5. A log without `[DEBUG]` lines produces a visible warning rather than silent blanks.
6. Zone names and acts are both recorded for a zone whose entry is followed by both kinds of
   `Set Source` line.
7. The event log is never mutated: rebuilding derived tables changes `zones` and
   `characters` only.
8. The tab's labels come from i18n with English and Russian present.

## Out of scope

- Pace norms, personal medians, rule-based advice — R2.
- The character panel screenshot and its vision model — R4.
- Voice — R5.
- Importing the Python database. The log reproduces it.
- Any write to the game's files. The log is opened read-only.
