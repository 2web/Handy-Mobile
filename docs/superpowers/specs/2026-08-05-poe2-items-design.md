# Path of Exile 2 Items — Design

**Date:** 2026-08-05
**Status:** Approved (design), pending spec review

## Goal

Bring item capture and parsing for Path of Exile 2 into Handy: paste (or copy in-game) an
item, get it parsed into structure and stored, so later features can reason about the
player's gear.

This is a port. A working Python implementation exists in a separate `poe2-helper`
project, complete with 266 passing tests and two fixtures taken from the player's real
items. The parser design below is not new — it was validated against the real game. What
is new is the language, the host application, and the storage layer.

## Context: why this moves into Handy

`poe2-helper` is a Python/FastAPI program that reads the game's `Client.txt`, reconstructs
character progress, derives the player's personal pace norms, and gives rule-based advice.
Four projects shipped: A (progress tracker), C1 (rule advisor), E1 (item capture).

The decision was made to move the whole thing into Handy and rewrite it in Rust. Handy
already provides everything the port needs — `rusqlite` with migrations, `regex`,
`chrono`, `reqwest`, a tray, global shortcuts, clipboard access, an LLM client with
structured output, and settings storage including API keys.

One motive stated for the rewrite was speed. That motive does not hold: the Python version
rebuilds 14,474 events in 2.5 seconds and parses an item in microseconds. The real benefits
are different and they are genuine — a single binary with no Python on the user's machine,
one window and one tray instead of two programs, and direct access to system APIs that the
later screenshot feature will need.

### The port is split into five projects

- **R1. Log and state** — read `Client.txt`, events, character state. (Port of project A.)
- **R2. Norms and advice** — personal pace, rule-based advice. (Port of C1.)
- **R3. Items** — this document. (Port of E1.)
- **R4. Character panel from a screenshot** — capture, crop, vision model, numbers.
- **R5. Voice** — spoken answers, on top of the dialogue tutor.

R3 goes first by the user's decision: R4 wants to cross-check recognised resistances
against what the equipped gear actually grants, and that check needs items in place.

**This document describes only R3.**

During the transition the two programs know different things — Handy knows items, the
Python version knows progress. That is expected and ends with R1.

## Data format: what the game actually gives

Hovering an item and pressing `Ctrl+C` puts a full textual description on the clipboard.
Verified against the player's real items:

```
Item Class: Sceptres
Rarity: Rare
Wrath Call
Rattling Sceptre
--------
Quality: +20% (augmented)
Spirit: 166 (augmented)
--------
Requires: Level 44
--------
Sockets: S
--------
Item Level: 58
--------
Grants Skill: Level 14 Skeletal Warrior Minion
--------
{ Prefix Modifier "Count's" (Tier: 4) }
50(45-50)% increased Spirit
{ Prefix Modifier "Counselor's" (Tier: 6) }
16(15-18)% increased Spirit
+22(21-24) to maximum Mana
{ Suffix Modifier "of the Overseer" (Tier: 2) — Minion, Gem }
+3 to Level of all Minion Skills
{ Crafted Suffix Modifier "of the Stars" — Minion }
Minions have 47(40-49)% increased Magnitude of Damaging Ailments
```

Five properties of this format drive the parser design. All five are already handled by the
Python implementation and must survive the port.

**Sections are separated by a line of dashes, and their number and order vary per item.**
Parsing is driven by section *content*, never by section index.

**One modifier can produce several lines.** The `Counselor's` prefix granted both spirit and
mana. Parsing must work in blocks — "a header in braces plus every line until the next
header" — not line by line. Line-by-line parsing turns one modifier into two and is painful
to unwind later.

**Values carry their roll range:** `50(45-50)% increased Spirit`. Both the rolled value and
the tier bounds are visible, which is what lets the program say "this prefix rolled at the
top of its tier" instead of merely "+50% spirit". A single line may carry two ranges at
once: `15(11-16) to 23(17-23) Physical Thorns damage`.

**Runes are a separate source of properties and cannot be skipped.** Lines like
`+5% to all Elemental Resistances (rune)` form their own section, are marked with a `(rune)`
suffix, and have no braces, tier, or range. In the body armour fixture the runes contribute
+18% fire and +5% all elemental — a third of the item's entire contribution. They are stored
with kind `rune`.

**Defensive properties are open-ended.** Besides armour and energy shield the armour fixture
carries `Runic Ward`, which appears in no older reference. The properties block is therefore
parsed as a set of key–value pairs, not against a fixed list of fields. An unknown key is
kept, not discarded.

### Dependence on a game setting

Brace blocks appear only when the game's advanced item descriptions are enabled. The player
has them on. With them off, only bare modifier text remains — no kind, name, tier, or tags.

The parser must work in both modes: the simple mode extracts less but does not fail. The
mode is detected by the presence of lines starting with `{`, and recorded on the item so the
UI can suggest turning the setting on.

## Scope & decisions (locked with user)

- **Sidebar section.** A "Path of Exile 2" entry in the existing sidebar, next to Settings
  and History. No separate window, no overlay.
- **Separate database file** `poe2.db` in the app data directory, with its own migrations.
  Game data has nothing to do with dictation history; a separate file can be deleted and
  rebuilt without touching Handy's own data, and it keeps upstream merges clean.
- **All game code lives in new files.** Existing files get the minimum: one module
  declaration, command registrations, one sidebar entry, i18n keys.
- **i18n from the start.** The section uses Handy's existing i18n with English as the source
  locale and Russian alongside it; other locales fall back.
- **Clipboard watching stays off by default** and is enabled explicitly.
- **No global keyboard hooks and no hotkey in the first version.** Polling the clipboard once
  a second is enough; a Handy shortcut is a later convenience.
- **The Python project gets archived** once the whole port is finished, not before.

## Architecture

```
item text ──> poe2::items (parser) ──> poe2::store ──> poe2.db
  paste or                                  │
  clipboard                                 v
                                    ItemsPage (React)
```

### New files

| File | Responsibility |
|---|---|
| `src-tauri/src/poe2/mod.rs` | module wiring |
| `src-tauri/src/poe2/items.rs` | parsing text into structure; **every item regex lives here and only here** |
| `src-tauri/src/poe2/store.rs` | SQLite schema, migrations, reads and writes |
| `src-tauri/src/poe2/clipboard_watch.rs` | polling loop and the strict item test |
| `src-tauri/src/poe2/commands.rs` | Tauri commands exposed to the frontend |
| `src/components/poe2/ItemsPage.tsx` | the sidebar section |
| `src-tauri/tests/fixtures/poe2/*.txt` | the two real items, copied verbatim |

### Edits to existing files

| File | Edit |
|---|---|
| `src-tauri/src/lib.rs` | `mod poe2;` plus the new commands in `collect_commands!` |
| `src/components/Sidebar.tsx` | one entry in `SECTIONS_CONFIG` |
| `src/i18n/locales/en/translation.json`, `ru/translation.json` | new `poe2.*` keys |

`items.rs` is a pure module: no I/O, no network, no database. That boundary is what makes
the parser testable against fixtures and is inherited from the Python design.

Commands are registered through the existing `tauri-specta` builder, so the Rust structs
reach the frontend as generated TypeScript types in `bindings.ts` — no hand-written DTOs.

### Enabling the section

The sidebar config already supports `enabled: (settings) => ...`, used by post-processing
and debug. The game section follows that pattern through a new boolean setting
`poe2_enabled`, defaulting to false, with its toggle in Advanced settings and a
`change_poe2_enabled_setting` command alongside the existing `change_*_setting` family.
Handy is a dictation tool for most of its users; a game panel should not appear uninvited.

Clipboard watching gets its own setting, `poe2_clipboard_watch`, also defaulting to false
and only reachable once the section is enabled. Two separate switches rather than one:
showing the section is harmless, while reading the clipboard in the background is the part
the user must knowingly agree to.

## Data model

The rule from the Python version carries over unchanged: **raw text is the source of truth,
structure is derived.**

`items` — `id`, `raw_hash`, `captured_ts`, `raw_text`, `source` (`paste` or `clipboard`),
`item_class`, `rarity`, `name`, `base_type`, `item_level`, `requires_level`, `quality`,
`sockets`, `properties` (JSON), `requirements` (JSON), `advanced`.

`item_mods` — `item_id`, `position`, `effect_index`, `kind`, `mod_name`, `tier`, `tags`
(JSON), `text`, `value`, `value_min`, `value_max`. Primary key
`(item_id, position, effect_index)`.

**A row is one effect, not one modifier.** Effects belonging to the same modifier share a
`position` and differ by `effect_index`, and they carry the same `kind`, `mod_name`, `tier`
and `tags`. This is how the format's central property is expressed in a flat table: a prefix
may produce several lines and still be one modifier. The parser returns one `ItemMod` with
several effects; the shared `position` says the same thing in SQL.

**The value columns hold the first value of the effect.** `15(11-16) to 23(17-23)` carries
two; the second is recoverable by reparsing the raw text, and the first consumer — summing
resistances — never needs it, because a resistance is always a single number.

Raw text is stored whole and never modified. Structure is recomputed from it by the
`poe2_rebuild_items` command, exposed to the frontend as a button in the section and
returning the number of items reparsed. When the parser gets smarter — and it will — stored
items reparse themselves with no re-pasting.

`kind` is stored as a lowercase string (`prefix`, `suffix`, `implicit`, `crafted`, `rune`,
`unknown`), which is what `ModKind` serialises to. The database holds text rather than an
integer discriminant so that a hand-run `SELECT` stays readable and adding a kind later does
not renumber the existing ones.

The same text pasted twice does not create a second record: the key is a SHA-256 hash of the
raw text.

## Rust types

```rust
pub struct ParsedItem {
    pub raw_text: String,
    pub item_class: Option<String>,
    pub rarity: Option<String>,
    pub name: Option<String>,
    pub base_type: Option<String>,
    pub item_level: Option<i64>,
    pub requires_level: Option<i64>,
    pub requirements: BTreeMap<String, i64>,
    pub quality: Option<i64>,
    pub sockets: Option<String>,
    pub properties: BTreeMap<String, String>,
    pub advanced: bool,
    pub mods: Vec<ItemMod>,
}

pub struct ItemMod {
    pub kind: ModKind,          // Prefix | Suffix | Implicit | Crafted | Rune | Unknown
    pub name: Option<String>,
    pub tier: Option<i64>,
    pub tags: Vec<String>,
    pub effects: Vec<ModEffect>,
}

pub struct ModEffect {
    pub text: String,
    pub values: Vec<ModValue>,
}

pub struct ModValue {
    pub value: f64,
    pub value_min: Option<f64>,
    pub value_max: Option<f64>,
}
```

`BTreeMap` rather than `HashMap` so that serialised properties and requirements have a
stable order — otherwise the JSON stored in SQLite differs between runs on identical input,
and diffing a reparse becomes noise.

A modifier header enclosed in braces that cannot be parsed in detail still counts as a
header, with kind `Unknown`. Turning it into an effect would write game bookkeeping into the
modifier's own text.

## Frontend

One page, structured like the Python version's section:

- a textarea whose `paste` event carries the text — no system clipboard access needed, the
  browser hands it over;
- a status line: saved / already saved / not an item;
- a list of stored items, newest first, each with name, base type, item level, and its
  modifier lines with tier;
- runes marked apart from affixes — a rune can be pulled out and moved to another item, and
  the player must be able to tell them apart;
- a hint when `advanced` is false, suggesting the game setting be turned on.

All text goes through `textContent`-equivalent rendering. Item text comes from the game and
never becomes markup.

## Clipboard watching

Polling, not interception. No global keyboard hooks: they look like a keylogger to
antivirus software and invite questions about the game's rules. Reading the clipboard once a
second is enough.

- **Strict test.** Only text starting with `Item Class:` is processed. Everything else is
  ignored silently — a password copied out of a password manager must never reach the
  parser.
- **Nothing is ever written to the clipboard.** The module only reads.
- **Off by default**, enabled explicitly in settings.
- **Nothing leaves the machine.** Parsing is entirely local.

Handy already reads the clipboard through `tauri-plugin-clipboard-manager`
(`clipboard.read_text()`), so no new dependency is needed.

Manual pasting always works and is the fallback when watching is off or unavailable.

## Failure behaviour

| Situation | Behaviour |
|---|---|
| Pasted text is not an item | Clear message, nothing stored |
| Advanced descriptions disabled | Parsing proceeds, `advanced` = false, UI suggests the setting |
| Unknown section | Skipped, parsing continues |
| Unknown modifier | Stored as text, no value parsing |
| Same item pasted again | The already-stored record is returned |
| Clipboard unreadable | Watching switches off with a message; manual pasting still works |

The unknown must never break parsing. The game adds items and affixes every league, and a
parser that dies on a new modifier is useless within a month.

## Testing

The port's real risk is a silent regression: the Rust parser reads the same files slightly
differently and nobody notices. The defence is cheap and mandatory.

- **Both fixtures move over verbatim**, and the assertions from the Python test suite move
  with them: the same expected values, written as Rust tests. This is a transcription of
  expectations, not a live cross-check between the two implementations — the Python project
  is being retired, so a permanent bridge between them would be scaffolding with no future.
  Anything the Rust parser gets wrong shows up as a fixture assertion that fails.
- `items.rs` — unit tests over the fixtures. Specifically: a multi-line modifier parses into
  one modifier with several effects; a value with a range decomposes into value plus bounds;
  two ranges in one line yield two values; the simple format without braces parses without
  failing; runes are kept with kind `rune`.
- `store.rs` — roundtrip tests over an in-memory database: effects of one modifier share a
  position, tags survive, the same item twice stores once, reparse replaces structure but
  not raw text.
- `clipboard_watch.rs` — tested with an injected reader, never the real system clipboard.
- No test depends on a running game.

## Definition of done

1. The sceptre parses completely: class, rarity, name, base, item level, requirements,
   quality, and four modifiers with kinds, tiers, tags and ranges.
2. `Counselor's` yields one modifier with two effects, not two modifiers.
3. An item pasted twice is stored once.
4. Text without `Item Class:` is rejected with a clear message and stores nothing.
5. Clipboard watching is off by default, enabled explicitly, processes only text starting
   with `Item Class:`, and never writes to the clipboard.
6. A rebuild command reparses stored items from raw text.
7. The body armour's two runes are stored with kind `rune` and are visually distinct from
   affixes in the UI.
8. The section is hidden until enabled, and its labels come from i18n with English and
   Russian present.

## Out of scope

- The game log, character state, pace norms, advice — R1 and R2.
- The character panel screenshot and its vision model — R4.
- Voice — R5.
- Comparing two items and advising which to wear.
- Unique items, gems, flasks, currency. Each is a distinct shape and none is confirmed by a
  sample. They will not crash the parser — unknown content is stored as text — but they will
  parse thinly.
- Archiving the Python project. That happens once the whole port is done, not after R3.
