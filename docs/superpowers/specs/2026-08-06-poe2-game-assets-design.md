# Path of Exile 2 Game Assets — Design

**Date:** 2026-08-06
**Status:** Approved (design), pending spec review

## Goal

Read the game's own art out of its archives so the Equipment tab can look like the game:
the inventory doll drawn with the game's slot frames, and every captured item shown with its
real icon.

## Context

Three features have shipped into Handy: item capture, the log tracker, and the resistance
calculator. The calculator's Equipment tab currently lists resistances as text. The player
asked for it to be graphical and to look like the game, with icons taken from the game
itself rather than drawn by hand.

That request turns out to be a subsystem rather than a detail, so it is its own project. It
runs before the graphical tab: building the doll first and re-skinning it later would mean
laying out a page twice.

## Why this is the riskiest project so far

The three previous ports had a working Python implementation to translate. This one has
none. It depends on:

- **A file format nobody documents.** `Bundles2/_.index.bin` and the `.bundle.bin` files
  have no official specification. Their layout is known only through community
  reimplementations, and Grinding Gear Games is free to change it in any patch.
- **A compression codec that is proprietary.** The bundles are Oodle Leviathan, and the game
  does not ship the codec as a separate library — it is linked into the executable, so it
  cannot be called. Decoding depends on
  [`oozextract`](https://github.com/lvlvllvlvllvlvl/oozextract), a clean-room Rust port of an
  open-source reimplementation.
- **Table schemas maintained outside the game.** `.datc64` files store rows with no column
  descriptions. That column 13 of `BaseItemTypes` means `ItemVisualIdentity` is knowledge
  held by [dat-schema](https://github.com/poe-tool-dev/dat-schema), a community project, and
  it shifts as the game changes.

Any of those three can break on a patch day, and the failure will look like missing icons
rather than an error. The design below therefore treats missing art as ordinary, not
exceptional.

## Scope & decisions (locked with user)

- **Extraction is automatic.** No button. When the tab needs art it does not have, it
  extracts it. The player is never asked to run a step.
- **Without the game, the Equipment tab still works** — resistances, the penalty and the
  gaps are computed from item text already in the database and never needed art. Only the
  doll is absent, replaced by a line naming the path that was tried.
- **No hand-drawn fallback icons.** The interface is the game's art or nothing.
- **The interface chrome comes from the game too** — slot frames and the doll's background,
  not only the item icons.
- **The two table schemas are vendored**, not downloaded: `BaseItemTypes` and
  `ItemVisualIdentity` only, a couple of kilobytes out of the full 2.5 MB. The program stays
  entirely offline, which every previous project also committed to.
- **Three new crates**, the first external dependencies any of these projects have added.

## The chain from an item to a picture

```
StoredItem.base_type            "Sapphire Ring"
        │
        ▼  BaseItemTypes.datc64, column Name → row
BaseItemTypes row
        │
        ▼  column ItemVisualIdentity (foreignrow)
ItemVisualIdentity row
        │
        ▼  column DDSFile
"Art/2DItems/Rings/SapphireRing.dds"
        │
        ▼  Bundles2 index: path → bundle + offset
compressed bytes
        │
        ▼  oozextract (Oodle Leviathan)
.dds bytes
        │
        ▼  image_dds (BC decode) + image (PNG encode)
PNG in the icon cache
```

Every step can fail independently, and each failure means one missing icon — never a broken
tab.

The alternative to reading `.dat` was guessing the file name from the base type
(`Sapphire Ring` → `SapphireRing.dds`). It needs no schemas and no `.dat` parsing, but it is
a guess: it silently produces the wrong icon whenever the game's naming differs. The
explicit link is worth the extra layer.

## New dependencies

| Crate | Purpose | Note |
|---|---|---|
| `oozextract` | Oodle Leviathan decompression | MIT, pure Rust, no proprietary library |
| `image_dds` | decoding BC1/BC3/BC7 textures | **decode features only** |
| `image` | writing PNG | already ubiquitous |

`image_dds` can also encode, and encoding pulls in Intel ISPC, which does not build
everywhere. Only its decoding features may be enabled, or Handy stops building on somebody
else's machine for a reason unrelated to this feature.

## Architecture

Everything lives in `poe2-core`, so it stays testable without Tauri:

| File | Responsibility |
|---|---|
| `src/assets/mod.rs` | module wiring |
| `src/assets/bundles.rs` | the index and bundle formats; path → bytes |
| `src/assets/dat.rs` | `.datc64` rows and the two vendored schemas |
| `src/assets/icons.rs` | base type → DDS path → PNG, and the cache |
| `src/assets/schema/*.json` | the two vendored table schemas |

The Tauri side gains one command to ask for an icon and a background task that fills the
cache; the game's directory is derived from the already-configured `Client.txt` path, which
sits in `logs/` inside it.

## The cache

Extracted PNGs live beside `poe2.db` in the app data directory, named by the base type they
belong to. A cached icon is served without touching the archives at all — the index alone is
114 MB compressed, and reopening it per icon would be absurd.

The cache is disposable: deleting it costs one re-extraction. It carries a version marker so
a change in how icons are produced invalidates it rather than serving stale pictures.

## Failure behaviour

| Situation | Behaviour |
|---|---|
| Game not installed at the derived path | Tab shows resistances, no doll, and names the path tried |
| Index format not understood | Extraction disabled, the reason logged once, resistances unaffected |
| A base type absent from `BaseItemTypes` | That item shows no icon; every other icon still works |
| `ItemVisualIdentity` missing or its DDS path empty | Same — one icon absent |
| A texture in a format `image_dds` cannot decode | Same, and the format is logged so it can be added |
| The cache directory is unwritable | Icons are extracted per session and not persisted |
| A patch changes the schema | Icons stop being found; resistances, progress and capture all keep working |

Nothing in this feature may take down anything that already works. That is the whole reason
the calculator does not depend on it.

## Testing

`.datc64` parsing and the bundle index are the parts worth testing hard, and both need real
files, which cannot go in the repository — they are the game's.

- **Format tests run against the player's installed game** and are marked `#[ignore]`, the
  same way the log tracker's real-log test is: they never run in the ordinary suite and never
  fail on a machine without the game.
- **Unit tests use hand-built byte buffers** for the row-splitting and offset arithmetic,
  which is where the errors live.
- **One end-to-end ignored test**: from `"Sapphire Ring"` to a PNG whose dimensions are
  plausible, over the real installation.
- The cache is tested with a temporary directory, never the real one.

## Definition of done

1. Given the installed game, an icon can be produced for each of the player's nine captured
   items, or a specific reason logged for any that cannot.
2. The slot frames and doll background are extracted from the game's UI art.
3. A second request for the same icon is served from the cache without opening any archive.
4. With the game absent, the Equipment tab still shows resistances, the penalty and the gaps,
   and names the path it tried.
5. A patch that breaks the schema degrades to missing icons, never to a broken tab or a
   failed build.
6. `image_dds` is configured for decoding only, and `cargo build` succeeds from a clean
   checkout.
7. No network access anywhere in the feature.

## Out of scope

- The graphical doll itself — that is the next project, which consumes this one.
- Models, sounds, and every table beyond the two named.
- Downloading or updating schemas.
- Redistributing any extracted art. It is written to the player's own app data, and nothing
  extracted goes into the repository.

## First, a spike

The index format is the one part that could turn out to be impractical, and no amount of
planning substitutes for opening the file. Before any implementation plan is written, a
throwaway spike must answer three questions against the player's real installation:

1. Does `oozextract` decompress `_.index.bin`?
2. Can the path list and the bundle records be located inside the decompressed index?
3. Can one known file — a slot frame — be pulled out and decoded to a PNG?

If the spike fails, the design changes rather than the plan being executed on hope. Its code
is thrown away either way; what it produces is knowledge and a handful of confirmed
constants.
