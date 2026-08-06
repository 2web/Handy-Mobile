# Path of Exile 2 Game Assets Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `poe2-core` learns to read the game's own archives — decompress bundles, look up files by path, walk the item tables — so a captured item's real icon and the game's inventory chrome land as PNGs in a local cache, ready for the graphical Equipment tab that comes next.

**Architecture:** A new `assets` module in `poe2-core`, pure of Tauri: `bundles.rs` (the compressed container format), `index.rs` (path → file location), `dat.rs` (`.datc64` tables against vendored schemas), `icons.rs` (the chain from a base type to a cached PNG). The `handy` crate adds two commands and a background fill task. Everything game-dependent is verified by `#[ignore]` tests against the player's installation; everything else is unit-tested on hand-built bytes.

**Tech Stack:** Rust 2021; new crates `oozextract` 0.5 (Oodle Leviathan, pure Rust, MIT), `image_dds` 0.7 (**decode only**), `image` 0.25 (PNG). Tauri 2.11 on the app side.

## Global Constraints

- **Every format constant below was measured on 2026-08-06 against the player's real installation** by a spike, not taken from documentation (none exists). If code and a constant disagree, suspect the code first, then re-verify against the game before changing the constant.
- **New crates are allowed in this project — exactly three:** `oozextract`, `image_dds`, `image`. `image_dds` must be declared `default-features = false, features = ["ddsfile", "image"]`: its encode path pulls Intel ISPC, which does not build everywhere, and a Handy checkout that stops building for an icon feature is unacceptable.
- **No network access anywhere.** Schemas are vendored in the repository, never downloaded.
- **Nothing here may break what already ships.** The resistance calculator, the tracker and item capture never call into `assets`; a failure in this module is a missing icon, never a broken tab, a panic, or a failed build.
- `poe2-core` still contains **no Tauri types**. Game-directory paths come in as arguments.
- **Game files are opened read-only.** Nothing is ever written inside the game directory; extracted PNGs go to the app-data cache only. Nothing extracted is ever committed to the repository.
- **Tests that need the game are `#[ignore]`d** (like the existing `real_log` test) and skip cleanly with a printed reason when the game is absent. The ordinary suite must stay green on a machine without the game.
- **Tests run in `poe2-core`:** `cd src-tauri/poe2-core && cargo test`. Never `cargo test` in `src-tauri` — that binary cannot start on this machine (`STATUS_ENTRYPOINT_NOT_FOUND`). Check the `handy` crate with `cd src-tauri && cargo build`.
- Only these pre-existing files may be edited: `src-tauri/poe2-core/Cargo.toml`, `src-tauri/poe2-core/src/lib.rs`, `src-tauri/src/poe2/mod.rs`, `src-tauri/src/poe2/commands.rs`, `src-tauri/src/lib.rs`. Everything else is new.
- Format with `cargo fmt` in both crates; revert unrelated drift (`actions.rs`, `tray.rs`); leave a churned `src/bindings.ts` unstaged except where a task says to commit it.
- Commit messages in English, `feat(poe2):` / `test(poe2):` style. No Co-Authored-By trailer.

---

## Measured constants — the spike's findings, all confirmed against the real game

**Game root** (derived at runtime from the configured `Client.txt` path, two levels up):
`C:\Program Files (x86)\Steam\steamapps\common\Path of Exile 2`

**Bundle container** (`Bundles2/_.index.bin` and every `*.bundle.bin`):

| offset | field | note |
|---|---|---|
| 0 | `uncompressed_size: u32` | |
| 4 | `total_payload: u32` | |
| 8 | `head_size: u32` | |
| 12 | `encoder: u32` | **12 = Oodle Leviathan** — the only value seen |
| 20 | `uncompressed: u64` | authoritative size |
| 28 | `total_payload: u64` | |
| 36 | `block_count: u32` | |
| 40 | `granularity: u32` | 262144 — block size before compression |
| 60 | `block_sizes: [u32; block_count]` | compressed size of each block |
| after | the blocks, back to back | each decompresses to `granularity`, last to the remainder |

**Decompressed index layout**, in order:
1. `bundle_count: u32`; per bundle: `name_len: u32`, name bytes (no `.bundle.bin` suffix), `uncompressed_size: u32`
2. `file_count: u32`; per file, 20 bytes: `path_hash: u64`, `bundle_index: u32`, `offset: u32`, `size: u32`
3. `path_rep_count: u32`; per record, 20 bytes: `hash: u64`, `payload_offset: u32`, `payload_size: u32`, `payload_recursive_size: u32`
4. the remainder is a **nested bundle** holding the encoded path list

**Path hashing: MurmurHash64A, seed `0x1337b33f`, over the lowercased path.** Verified on 20,000 consecutive paths — all matched. FNV1a-64 does not match in any variant. Known-good vector for tests: `"audio/haptics/icenova_impact.wav"` → `0x76d511f0bb2b3ec4`.

**Path encoding** (inside the nested bundle): a stream of `u32` commands. `0` toggles phase and, on entering base phase, clears the accumulated bases. Any other value `c`: index `c − 1` into the bases, followed by a NUL-terminated string appended to that base (or used alone when the index has no entry). In base phase the result is pushed onto the bases; otherwise it is emitted as a full path. On the real index this yields exactly 4,227,110 paths — equal to `file_count`.

**On this installation:** 61,212 bundles, 4,227,110 files, 95,255 path-rep records, paths blob 124,963,297 bytes.

**Confirmed paths:**
- tables: `data/balance/baseitemtypes.datc64`, `data/balance/itemvisualidentity.datc64` (localised variants live under `data/balance/russian/` etc. — **use the base English one**: captured item text is English)
- item icons: `art/2ditems/…` (sample extracted end-to-end: `art/2ditems/rings/ventorsgamble.dds`, 80×80, DX10)
- inventory chrome: `art/textures/interface/2d/2dart/uiimages/ingame/inventorysquare.dds` — the slot cell background

**A fact that justifies the whole `.dat` route:** there is no `art/2ditems/rings/sapphirering.dds` — only `sapphireringalt.dds` variants. Guessing the file name from the base type would silently miss; the table's `DDSFile` column is the only reliable link.

**`.datc64` row format** (community knowledge, validated in Task 3 by the magic-boundary check): `row_count: u32` at offset 0, then `row_count` fixed-width rows, then a variable-data section that begins with the 8-byte magic `BB BB BB BB BB BB BB BB`. Column widths in bytes: `bool` 1, `u16` 2, `i32`/`u32`/`f32` 4, `enumrow` 4, `string` 8 (u64 offset **relative to the magic**), `row` 8, `foreignrow` 16 (u64 rowid + u64 padding; `0xFEFEFEFEFEFEFEFE` = null), any `array` 16 (u64 count + u64 offset). Strings in the variable section are UTF-16LE terminated by four zero bytes.

**The two tables, PoE2 variants** (`validFor = 2` in the community schema — **the PoE1 variants have different column sets**; vendoring the wrong one reads garbage):
- `BaseItemTypes`: 34 columns; the ones used are `[4] Name: string` and `[13] ItemVisualIdentity: foreignrow`
- `ItemVisualIdentity`: 89 columns; the one used is `[1] DDSFile: string`

---

## File structure

| File | Responsibility |
|---|---|
| `src-tauri/poe2-core/src/assets/mod.rs` | module wiring |
| `src-tauri/poe2-core/src/assets/bundles.rs` | the container: header, blocks, decompression |
| `src-tauri/poe2-core/src/assets/index.rs` | index records, path decoding, murmur hash, path → location |
| `src-tauri/poe2-core/src/assets/dat.rs` | `.datc64` rows, string/foreignrow readers, boundary validation |
| `src-tauri/poe2-core/src/assets/schema.rs` | the two vendored column lists as Rust consts |
| `src-tauri/poe2-core/src/assets/icons.rs` | base type → DDS path → PNG; the versioned cache |
| `src-tauri/poe2-core/src/assets/acceptance.rs` | ignored end-to-end tests against the real game |
| `src-tauri/src/poe2/assets_task.rs` | background fill, `poe2://icons-changed` |

One amendment to the spec's file table: schemas are vendored as **Rust consts** (`schema.rs`), not JSON files. Same review surface, same "vendored not downloaded" guarantee, and no runtime parse step that could fail. The spec's intent was the vendoring, not the serialisation format.

---

## Task 1: The bundle container

**Files:**
- Create: `src-tauri/poe2-core/src/assets/mod.rs`, `src-tauri/poe2-core/src/assets/bundles.rs`
- Modify: `src-tauri/poe2-core/Cargo.toml`, `src-tauri/poe2-core/src/lib.rs`

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `assets::bundles::BundleHead { pub uncompressed: u64, pub block_count: usize, pub granularity: usize, pub encoder: u32, pub block_sizes: Vec<usize>, pub data_offset: usize }`
  - `assets::bundles::BundleHead::parse(raw: &[u8]) -> Result<BundleHead, AssetError>`
  - `assets::bundles::decompress(raw: &[u8]) -> Result<Vec<u8>, AssetError>`
  - `assets::AssetError` — enum `Truncated { need: usize, have: usize }`, `UnknownEncoder(u32)`, `Decompress(String)`, `NotFound(String)`, `BadTable(String)`, `BadTexture(String)`, `Io(String)`; implements `Display` + `std::error::Error`

- [ ] **Step 1: Add the dependency**

In `src-tauri/poe2-core/Cargo.toml`, append to `[dependencies]`:

```toml
oozextract = "0.5"
```

- [ ] **Step 2: Write the failing tests**

Create `src-tauri/poe2-core/src/assets/bundles.rs` containing only this test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// A syntactically valid header for 2 blocks of a 300000-byte payload.
    /// Compression is fake — only `parse` is exercised here; `decompress` gets
    /// real data only in the ignored acceptance tests, because Leviathan data
    /// cannot be built by hand.
    fn header(encoder: u32, uncompressed: u64, blocks: &[u32], granularity: u32) -> Vec<u8> {
        let mut b = vec![0u8; 60];
        b[0..4].copy_from_slice(&(uncompressed as u32).to_le_bytes());
        b[12..16].copy_from_slice(&encoder.to_le_bytes());
        b[20..28].copy_from_slice(&uncompressed.to_le_bytes());
        b[36..40].copy_from_slice(&(blocks.len() as u32).to_le_bytes());
        b[40..44].copy_from_slice(&granularity.to_le_bytes());
        for size in blocks {
            b.extend_from_slice(&size.to_le_bytes());
        }
        // Compressed payload placeholder so offsets are in bounds.
        b.extend(std::iter::repeat(0u8).take(blocks.iter().sum::<u32>() as usize));
        b
    }

    #[test]
    fn a_real_shaped_header_parses() {
        let raw = header(12, 300_000, &[1000, 800], 262_144);
        let head = BundleHead::parse(&raw).unwrap();
        assert_eq!(head.encoder, 12);
        assert_eq!(head.uncompressed, 300_000);
        assert_eq!(head.block_count, 2);
        assert_eq!(head.granularity, 262_144);
        assert_eq!(head.block_sizes, vec![1000, 800]);
        assert_eq!(head.data_offset, 60 + 2 * 4);
    }

    #[test]
    fn truncated_input_is_an_error_not_a_panic() {
        let raw = header(12, 300_000, &[1000, 800], 262_144);
        for cut in [0usize, 10, 59, 61] {
            assert!(
                BundleHead::parse(&raw[..cut.min(raw.len())]).is_err(),
                "cut at {cut} must fail cleanly"
            );
        }
    }

    #[test]
    fn a_header_promising_more_blocks_than_it_carries_is_an_error() {
        let mut raw = header(12, 300_000, &[1000, 800], 262_144);
        // Claim 1000 blocks; the size table is nowhere near that long.
        raw[36..40].copy_from_slice(&1000u32.to_le_bytes());
        assert!(BundleHead::parse(&raw).is_err());
    }

    #[test]
    fn an_unknown_encoder_is_refused_by_decompress() {
        // The game has only ever shown encoder 12. Anything else means the
        // format moved under us, and guessing would corrupt output silently.
        let raw = header(7, 300_000, &[1000], 262_144);
        match decompress(&raw) {
            Err(AssetError::UnknownEncoder(7)) => {}
            other => panic!("expected UnknownEncoder(7), got {other:?}"),
        }
    }

    #[test]
    fn a_block_size_running_past_the_payload_is_an_error() {
        let mut raw = header(12, 300_000, &[1000, 800], 262_144);
        let len = raw.len();
        raw.truncate(len - 500); // payload now shorter than the size table claims
        assert!(decompress(&raw).is_err());
    }

    #[test]
    fn zero_blocks_yield_an_empty_output() {
        let raw = header(12, 0, &[], 262_144);
        assert_eq!(decompress(&raw).unwrap(), Vec::<u8>::new());
    }
}
```

- [ ] **Step 3: Run the tests to verify they fail**

```bash
cd src-tauri/poe2-core && cargo test assets::
```

Expected: compilation error — `file not found for module assets`.

- [ ] **Step 4: Write the implementation**

Create `src-tauri/poe2-core/src/assets/mod.rs`:

```rust
//! Reading the game's own archives.
//!
//! No format here is officially documented. Every constant was measured against
//! the player's real installation on 2026-08-06 (see the plan's "Measured
//! constants"); when code and constant disagree, re-verify against the game
//! before touching the constant.

pub mod bundles;

use std::fmt;

#[derive(Debug)]
pub enum AssetError {
    Truncated { need: usize, have: usize },
    UnknownEncoder(u32),
    Decompress(String),
    NotFound(String),
    BadTable(String),
    BadTexture(String),
    Io(String),
}

impl fmt::Display for AssetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AssetError::Truncated { need, have } => {
                write!(f, "truncated data: need {need} bytes, have {have}")
            }
            AssetError::UnknownEncoder(e) => write!(f, "unknown bundle encoder {e}"),
            AssetError::Decompress(e) => write!(f, "decompression failed: {e}"),
            AssetError::NotFound(p) => write!(f, "not in the archives: {p}"),
            AssetError::BadTable(e) => write!(f, "table refused: {e}"),
            AssetError::BadTexture(e) => write!(f, "texture refused: {e}"),
            AssetError::Io(e) => write!(f, "io: {e}"),
        }
    }
}

impl std::error::Error for AssetError {}
```

Insert above the `#[cfg(test)]` block in `src-tauri/poe2-core/src/assets/bundles.rs`:

```rust
//! The bundle container: a header, a table of block sizes, then Oodle-compressed
//! blocks of 256 KiB each (the last one shorter).

use crate::assets::AssetError;

/// The only encoder ever observed. Anything else fails loudly rather than
/// guessing: a wrong decompressor produces garbage that *parses*.
const ENCODER_LEVIATHAN: u32 = 12;
const HEADER_LEN: usize = 60;

fn u32_at(b: &[u8], o: usize) -> Result<u32, AssetError> {
    b.get(o..o + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or(AssetError::Truncated { need: o + 4, have: b.len() })
}

fn u64_at(b: &[u8], o: usize) -> Result<u64, AssetError> {
    b.get(o..o + 8)
        .map(|s| {
            let mut v = [0u8; 8];
            v.copy_from_slice(s);
            u64::from_le_bytes(v)
        })
        .ok_or(AssetError::Truncated { need: o + 8, have: b.len() })
}

#[derive(Debug, Clone)]
pub struct BundleHead {
    pub uncompressed: u64,
    pub block_count: usize,
    pub granularity: usize,
    pub encoder: u32,
    pub block_sizes: Vec<usize>,
    pub data_offset: usize,
}

impl BundleHead {
    pub fn parse(raw: &[u8]) -> Result<BundleHead, AssetError> {
        if raw.len() < HEADER_LEN {
            return Err(AssetError::Truncated { need: HEADER_LEN, have: raw.len() });
        }
        let encoder = u32_at(raw, 12)?;
        let uncompressed = u64_at(raw, 20)?;
        let block_count = u32_at(raw, 36)? as usize;
        let granularity = u32_at(raw, 40)? as usize;

        let mut block_sizes = Vec::with_capacity(block_count);
        for i in 0..block_count {
            block_sizes.push(u32_at(raw, HEADER_LEN + i * 4)? as usize);
        }
        Ok(BundleHead {
            uncompressed,
            block_count,
            granularity,
            encoder,
            block_sizes,
            data_offset: HEADER_LEN + block_count * 4,
        })
    }
}

/// Decompresses a whole bundle into memory.
pub fn decompress(raw: &[u8]) -> Result<Vec<u8>, AssetError> {
    let head = BundleHead::parse(raw)?;
    if head.encoder != ENCODER_LEVIATHAN {
        return Err(AssetError::UnknownEncoder(head.encoder));
    }

    let mut out = Vec::with_capacity(head.uncompressed as usize);
    let mut at = head.data_offset;
    for (i, size) in head.block_sizes.iter().enumerate() {
        let src = raw
            .get(at..at + size)
            .ok_or(AssetError::Truncated { need: at + size, have: raw.len() })?;
        let want = (head.uncompressed as usize - out.len()).min(head.granularity);
        let mut dst = vec![0u8; want];
        oozextract::Extractor::new()
            .read_from_slice(src, &mut dst)
            .map_err(|e| AssetError::Decompress(format!("block {i}: {e:?}")))?;
        out.extend_from_slice(&dst);
        at += size;
    }
    Ok(out)
}
```

Add `pub mod assets;` to `src-tauri/poe2-core/src/lib.rs`.

- [ ] **Step 5: Run the tests to verify they pass**

```bash
cd src-tauri/poe2-core && cargo test
```

Expected: PASS — the 168 existing tests plus 6 new ones.

- [ ] **Step 6: Format and commit**

```bash
cd src-tauri/poe2-core && cargo fmt
cd ../.. && git add src-tauri/poe2-core
git commit -m "feat(poe2): bundle container parsing and Oodle decompression"
```

---

## Task 2: The index — records, paths, hash

**Files:**
- Create: `src-tauri/poe2-core/src/assets/index.rs`
- Modify: `src-tauri/poe2-core/src/assets/mod.rs`

**Interfaces:**
- Consumes: `assets::bundles::decompress`, `assets::AssetError`.
- Produces:
  - `assets::index::murmur64a(data: &[u8], seed: u64) -> u64` and `assets::index::PATH_SEED: u64` = `0x1337b33f`
  - `assets::index::FileLocation { pub bundle: String, pub offset: usize, pub size: usize }`
  - `assets::index::IndexRecords { pub bundle_names: Vec<String>, pub files: Vec<(u64, u32, u32, u32)>, pub tail_offset: usize }`
  - `assets::index::parse_records(index: &[u8]) -> Result<IndexRecords, AssetError>` — over the **decompressed** index
  - `assets::index::decode_paths(payload: &[u8]) -> Vec<String>`
  - `assets::index::BundleIndex` with `load(index_file_bytes: &[u8]) -> Result<BundleIndex, AssetError>`, `find(&self, path: &str) -> Option<FileLocation>`, `paths_with_prefix(&self, prefix: &str) -> Vec<&str>`

`parse_records` is split out precisely so it can be unit-tested on hand-built bytes; `load` composes decompression, record parsing and path decoding and is exercised only by the ignored acceptance tests.

- [ ] **Step 1: Write the failing tests**

Create `src-tauri/poe2-core/src/assets/index.rs` with only this test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// The spike's confirmed vector: this exact path hashed to this exact value
    /// on the real index, where all 20,000 probed paths matched this scheme.
    #[test]
    fn murmur_matches_the_measured_vector() {
        assert_eq!(
            murmur64a(b"audio/haptics/icenova_impact.wav", PATH_SEED),
            0x76d511f0bb2b3ec4
        );
    }

    #[test]
    fn murmur_handles_tails_shorter_than_eight_bytes() {
        // Not a fixed vector — just that 1..8-byte inputs neither panic nor collide
        // with each other trivially.
        let mut seen = std::collections::HashSet::new();
        for len in 0..=8usize {
            assert!(seen.insert(murmur64a(&b"abcdefgh"[..len], PATH_SEED)));
        }
    }

    fn cmd(v: u32) -> Vec<u8> {
        v.to_le_bytes().to_vec()
    }
    fn s(text: &str) -> Vec<u8> {
        let mut b = text.as_bytes().to_vec();
        b.push(0);
        b
    }

    #[test]
    fn paths_decode_with_prefix_accumulation() {
        // Base phase: build "art/" then "art/rings/". Emit phase: attach names.
        let mut payload = Vec::new();
        payload.extend(cmd(0)); // enter base phase (clears bases)
        payload.extend(cmd(1)); // no base yet at index 0 -> the string alone
        payload.extend(s("art/"));
        payload.extend(cmd(1)); // base[0] = "art/" -> "art/rings/"
        payload.extend(s("rings/"));
        payload.extend(cmd(0)); // leave base phase
        payload.extend(cmd(2)); // base[1] = "art/rings/"
        payload.extend(s("a.dds"));
        payload.extend(cmd(2));
        payload.extend(s("b.dds"));

        assert_eq!(
            decode_paths(&payload),
            vec!["art/rings/a.dds".to_string(), "art/rings/b.dds".to_string()]
        );
    }

    #[test]
    fn reentering_base_phase_clears_the_bases() {
        let mut payload = Vec::new();
        payload.extend(cmd(0));
        payload.extend(cmd(1));
        payload.extend(s("first/"));
        payload.extend(cmd(0)); // leave
        payload.extend(cmd(2));
        payload.extend(s("x"));
        payload.extend(cmd(0)); // re-enter: bases cleared
        payload.extend(cmd(1));
        payload.extend(s("second/"));
        payload.extend(cmd(0));
        payload.extend(cmd(2));
        payload.extend(s("y"));

        assert_eq!(
            decode_paths(&payload),
            vec!["first/x".to_string(), "second/y".to_string()]
        );
    }

    #[test]
    fn truncated_path_payload_does_not_panic() {
        let mut payload = Vec::new();
        payload.extend(cmd(0));
        payload.extend(cmd(1));
        payload.extend(b"no trailing nul".to_vec()); // no terminator, ends mid-string
        let _ = decode_paths(&payload); // must simply return what it has
    }

    /// A hand-built decompressed index: 2 bundles, 3 files, 1 path-rep record.
    fn built_index() -> Vec<u8> {
        let mut b = Vec::new();
        b.extend(2u32.to_le_bytes());
        for name in ["bundles/alpha", "bundles/beta"] {
            b.extend((name.len() as u32).to_le_bytes());
            b.extend(name.as_bytes());
            b.extend(1000u32.to_le_bytes());
        }
        b.extend(3u32.to_le_bytes());
        for (hash, bundle, offset, size) in
            [(0x11u64, 0u32, 0u32, 10u32), (0x22, 1, 10, 20), (0x33, 1, 30, 5)]
        {
            b.extend(hash.to_le_bytes());
            b.extend(bundle.to_le_bytes());
            b.extend(offset.to_le_bytes());
            b.extend(size.to_le_bytes());
        }
        b.extend(1u32.to_le_bytes());
        b.extend([0u8; 20]); // one path-rep record, contents irrelevant here
        b.extend(b"TAIL"); // whatever follows is the nested paths bundle
        b
    }

    #[test]
    fn records_parse_and_the_tail_is_located() {
        let raw = built_index();
        let rec = parse_records(&raw).unwrap();
        assert_eq!(rec.bundle_names, vec!["bundles/alpha", "bundles/beta"]);
        assert_eq!(rec.files.len(), 3);
        assert_eq!(rec.files[1], (0x22, 1, 10, 20));
        assert_eq!(&raw[rec.tail_offset..], b"TAIL");
    }

    #[test]
    fn a_truncated_index_is_an_error_not_a_panic() {
        let raw = built_index();
        for cut in [0usize, 3, 10, 40, raw.len() - 5] {
            let _ = parse_records(&raw[..cut]); // Err is fine; a panic is not
        }
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cd src-tauri/poe2-core && cargo test assets::index
```

Expected: compilation error — `cannot find function murmur64a`.

- [ ] **Step 3: Write the implementation**

Insert above the `#[cfg(test)]` block in `src-tauri/poe2-core/src/assets/index.rs`:

```rust
//! The archive index: which file lives in which bundle, and under what name.

use std::collections::HashMap;

use crate::assets::bundles::decompress;
use crate::assets::AssetError;

/// The seed the game hashes archive paths with. Measured, not documented.
pub const PATH_SEED: u64 = 0x1337b33f;

/// MurmurHash64A. The game addresses files by this hash of the lowercased path.
pub fn murmur64a(data: &[u8], seed: u64) -> u64 {
    const M: u64 = 0xc6a4a7935bd1e995;
    const R: u32 = 47;
    let mut h = seed ^ (data.len() as u64).wrapping_mul(M);
    let chunks = data.chunks_exact(8);
    let tail = chunks.remainder();
    for c in chunks {
        let mut k = u64::from_le_bytes([c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7]]);
        k = k.wrapping_mul(M);
        k ^= k >> R;
        k = k.wrapping_mul(M);
        h ^= k;
        h = h.wrapping_mul(M);
    }
    let mut t = 0u64;
    for (i, b) in tail.iter().enumerate() {
        t |= (*b as u64) << (8 * i);
    }
    if !tail.is_empty() {
        h ^= t;
        h = h.wrapping_mul(M);
    }
    h ^= h >> R;
    h = h.wrapping_mul(M);
    h ^= h >> R;
    h
}

fn u32_at(b: &[u8], o: usize) -> Result<u32, AssetError> {
    b.get(o..o + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or(AssetError::Truncated { need: o + 4, have: b.len() })
}

fn u64_at(b: &[u8], o: usize) -> Result<u64, AssetError> {
    b.get(o..o + 8)
        .map(|s| {
            let mut v = [0u8; 8];
            v.copy_from_slice(s);
            u64::from_le_bytes(v)
        })
        .ok_or(AssetError::Truncated { need: o + 8, have: b.len() })
}

#[derive(Debug, Clone, PartialEq)]
pub struct FileLocation {
    pub bundle: String,
    pub offset: usize,
    pub size: usize,
}

pub struct IndexRecords {
    pub bundle_names: Vec<String>,
    /// (path_hash, bundle_index, offset, size)
    pub files: Vec<(u64, u32, u32, u32)>,
    /// Where the nested paths bundle begins in the decompressed index.
    pub tail_offset: usize,
}

/// Parses the three record blocks of an already-decompressed index.
pub fn parse_records(index: &[u8]) -> Result<IndexRecords, AssetError> {
    let mut cur = 0usize;
    let bundle_count = u32_at(index, cur)? as usize;
    cur += 4;
    let mut bundle_names = Vec::with_capacity(bundle_count);
    for _ in 0..bundle_count {
        let len = u32_at(index, cur)? as usize;
        let name = index
            .get(cur + 4..cur + 4 + len)
            .ok_or(AssetError::Truncated { need: cur + 4 + len, have: index.len() })?;
        bundle_names.push(String::from_utf8_lossy(name).into_owned());
        cur += 4 + len + 4; // name_len + name + uncompressed_size
    }

    let file_count = u32_at(index, cur)? as usize;
    cur += 4;
    let mut files = Vec::with_capacity(file_count);
    for _ in 0..file_count {
        files.push((
            u64_at(index, cur)?,
            u32_at(index, cur + 8)?,
            u32_at(index, cur + 12)?,
            u32_at(index, cur + 16)?,
        ));
        cur += 20;
    }

    let rep_count = u32_at(index, cur)? as usize;
    cur += 4 + rep_count * 20;
    if cur > index.len() {
        return Err(AssetError::Truncated { need: cur, have: index.len() });
    }

    Ok(IndexRecords { bundle_names, files, tail_offset: cur })
}

/// Decodes the segmented path list. Command 0 toggles the base phase (clearing
/// accumulated bases on entry); any other command `c` appends the following
/// NUL-terminated string to base `c - 1`, either accumulating it as a new base
/// or emitting it as a finished path.
pub fn decode_paths(payload: &[u8]) -> Vec<String> {
    let mut bases: Vec<String> = Vec::new();
    let mut out = Vec::new();
    let mut base_phase = false;
    let mut p = 0usize;
    while p + 4 <= payload.len() {
        let cmd = u32::from_le_bytes([payload[p], payload[p + 1], payload[p + 2], payload[p + 3]])
            as usize;
        p += 4;
        if cmd == 0 {
            base_phase = !base_phase;
            if base_phase {
                bases.clear();
            }
            continue;
        }
        let start = p;
        while p < payload.len() && payload[p] != 0 {
            p += 1;
        }
        if p >= payload.len() {
            break; // truncated final string: keep what we have
        }
        let s = String::from_utf8_lossy(&payload[start..p]).into_owned();
        p += 1;
        let full = match bases.get(cmd - 1) {
            Some(prefix) => format!("{prefix}{s}"),
            None => s,
        };
        if base_phase {
            bases.push(full);
        } else {
            out.push(full);
        }
    }
    out
}

/// The loaded index: every path, and a hash map from path hash to location.
pub struct BundleIndex {
    paths: Vec<String>,
    by_hash: HashMap<u64, (u32, u32, u32)>,
    bundle_names: Vec<String>,
}

impl BundleIndex {
    /// Loads from the raw on-disk bytes of `_.index.bin`.
    pub fn load(index_file_bytes: &[u8]) -> Result<BundleIndex, AssetError> {
        let index = decompress(index_file_bytes)?;
        let records = parse_records(&index)?;
        let paths = decode_paths(&decompress(&index[records.tail_offset..])?);
        let mut by_hash = HashMap::with_capacity(records.files.len());
        for (hash, bundle, offset, size) in &records.files {
            by_hash.insert(*hash, (*bundle, *offset, *size));
        }
        Ok(BundleIndex { paths, by_hash, bundle_names: records.bundle_names })
    }

    pub fn find(&self, path: &str) -> Option<FileLocation> {
        let hash = murmur64a(path.to_lowercase().as_bytes(), PATH_SEED);
        let (bundle, offset, size) = self.by_hash.get(&hash)?;
        Some(FileLocation {
            bundle: self.bundle_names.get(*bundle as usize)?.clone(),
            offset: *offset as usize,
            size: *size as usize,
        })
    }

    pub fn paths_with_prefix(&self, prefix: &str) -> Vec<&str> {
        self.paths
            .iter()
            .filter(|p| p.starts_with(prefix))
            .map(String::as_str)
            .collect()
    }
}
```

Add `pub mod index;` to `src-tauri/poe2-core/src/assets/mod.rs`.

- [ ] **Step 4: Run the tests to verify they pass**

```bash
cd src-tauri/poe2-core && cargo test
```

Expected: PASS — 7 new tests on top of the previous total.

- [ ] **Step 5: Format and commit**

```bash
cd src-tauri/poe2-core && cargo fmt
cd ../.. && git add src-tauri/poe2-core
git commit -m "feat(poe2): archive index, path decoding and murmur addressing"
```

---

## Task 3: `.datc64` tables and the vendored schemas

**Files:**
- Create: `src-tauri/poe2-core/src/assets/dat.rs`, `src-tauri/poe2-core/src/assets/schema.rs`
- Modify: `src-tauri/poe2-core/src/assets/mod.rs`

**Interfaces:**
- Consumes: `assets::AssetError`.
- Produces:
  - `assets::schema::Col` — enum `Bool, U16, I32, U32, F32, Enum, Row, Str, Foreign, Arr`; method `width(&self) -> usize` (1, 2, 4, 4, 4, 4, 8, 8, 16, 16)
  - `assets::schema::BASE_ITEM_TYPES: &[Col]` (34 entries) and named indices `pub const BIT_NAME: usize = 4;`, `pub const BIT_VISUAL_IDENTITY: usize = 13;`
  - `assets::schema::ITEM_VISUAL_IDENTITY: &[Col]` (89 entries) and `pub const IVI_DDS_FILE: usize = 1;`
  - `assets::dat::DatFile` with `parse(bytes: &[u8], schema: &[Col]) -> Result<DatFile, AssetError>`, `row_count(&self) -> usize`, `string_at(&self, row: usize, col_index: usize, schema: &[Col]) -> Option<String>`, `foreign_row(&self, row: usize, col_index: usize, schema: &[Col]) -> Option<u64>`

**The boundary check is the load-bearing part.** The variable-data magic must land exactly at `4 + row_count × row_width`. If it does not, the vendored schema no longer matches the game — a patch changed the table — and the file **refuses to load** (`BadTable`), which downstream turns into "icons missing" rather than garbage reads that look like data.

- [ ] **Step 1: Write the schemas**

Create `src-tauri/poe2-core/src/assets/schema.rs`:

```rust
//! The two table schemas this feature needs, vendored from the community's
//! dat-schema project (poe-tool-dev/dat-schema), PoE2 variants (`validFor: 2`)
//! as of 2026-08-06. The PoE1 variants of the same tables have different column
//! sets — vendoring the wrong one reads garbage, which is why these are pinned
//! rather than fetched.
//!
//! Only column *types* matter for byte offsets; names are noted where used.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Col {
    Bool,
    U16,
    I32,
    U32,
    F32,
    Enum,
    Row,
    Str,
    Foreign,
    Arr,
}

impl Col {
    /// Byte width in the fixed section of a .datc64 row.
    pub fn width(&self) -> usize {
        match self {
            Col::Bool => 1,
            Col::U16 => 2,
            Col::I32 | Col::U32 | Col::F32 | Col::Enum => 4,
            Col::Row | Col::Str => 8,
            Col::Foreign | Col::Arr => 16,
        }
    }
}

use Col::*;

/// data/balance/baseitemtypes.datc64 — 34 columns.
/// [4] = Name, [13] = ItemVisualIdentity.
pub const BASE_ITEM_TYPES: &[Col] = &[
    Str,     // 0  Id
    Foreign, // 1  ItemClass
    I32,     // 2  Width
    I32,     // 3  Height
    Str,     // 4  Name
    Str,     // 5  InheritsFrom
    I32,     // 6  DropLevel
    Foreign, // 7  FlavourText
    Arr,     // 8  Implicit_Mods
    I32,     // 9  SizeOnGround
    Foreign, // 10 SoundEffect
    Arr,     // 11 Tags
    Enum,    // 12 ModDomain
    Foreign, // 13 ItemVisualIdentity
    U32,     // 14 HASH32
    Arr,     // 15 VendorRecipe_AchievementItems
    Str,     // 16 Inflection
    Foreign, // 17 Equip_AchievementItem
    Bool,    // 18 IsCorrupted
    Arr,     // 19 Identify_AchievementItems
    Arr,     // 20 IdentifyMagic_AchievementItems
    Row,     // 21 FragmentBaseItemType
    Bool,    // 22
    Foreign, // 23 UncutGemSoundEffect
    Foreign, // 24
    Bool,    // 25
    Bool,    // 26 Unmodifiable
    Arr,     // 27 Achievement
    Foreign, // 28 ShopTag
    Foreign, // 29
    I32,     // 30
    Arr,     // 31
    Arr,     // 32
    Arr,     // 33
];

pub const BIT_NAME: usize = 4;
pub const BIT_VISUAL_IDENTITY: usize = 13;

/// data/balance/itemvisualidentity.datc64 — 89 columns. [1] = DDSFile.
pub const ITEM_VISUAL_IDENTITY: &[Col] = &[
    Str,     // 0  Id
    Str,     // 1  DDSFile
    Str,     // 2  AOFile
    Foreign, // 3  InventorySoundEffect
    U16,     // 4  HASH16
    Str,     // 5  AOFile2
    Arr,     // 6  MarauderSMFiles
    Arr,     // 7  RangerSMFiles
    Arr,     // 8  WitchSMFiles
    Arr,     // 9  DuelistDexSMFiles
    Arr,     // 10 TemplarSMFiles
    Arr,     // 11 ShadowSMFiles
    Arr,     // 12 ScionSMFiles
    Str,     // 13 MarauderShape
    Str,     // 14 RangerShape
    Str,     // 15 WitchShape
    Str,     // 16 DuelistShape
    Str,     // 17 TemplarShape
    Str,     // 18 ShadowShape
    Str,     // 19 ScionShape
    Foreign, // 20 TwoHandSoundType
    I32,     // 21
    Arr,     // 22 Pickup_AchievementItems
    Arr,     // 23 SMFiles
    Arr,     // 24 Identify_AchievementItems
    Str,     // 25 EPKFile
    Arr,     // 26 Corrupt_AchievementItems
    Bool,    // 27 IsAlternateArt
    Bool,    // 28
    Foreign, // 29 CreateCorruptedJewelAchievementItem
    Str,     // 30 AnimationLocation
    Str,     // 31
    Str,     // 32
    Str,     // 33
    Str,     // 34
    Str,     // 35
    Str,     // 36
    Str,     // 37
    Str,     // 38
    Str,     // 39
    Str,     // 40
    Str,     // 41
    Str,     // 42
    Bool,    // 43 IsAtlasOfWorldsMapIcon
    Bool,    // 44 IsTier16Icon
    Arr,     // 45
    Bool,    // 46
    Arr,     // 47
    Arr,     // 48
    Arr,     // 49
    Arr,     // 50
    Arr,     // 51
    Arr,     // 52
    Arr,     // 53
    Arr,     // 54
    Str,     // 55
    Str,     // 56
    Str,     // 57
    Str,     // 58
    Str,     // 59
    Str,     // 60
    Str,     // 61
    Str,     // 62
    Str,     // 63
    Enum,    // 64 Composition
    Foreign, // 65 UniqueStat1
    Foreign, // 66 UniqueStat2
    Foreign, // 67 UniqueStat3
    Foreign, // 68 UniqueStat4
    Arr,     // 69
    Arr,     // 70
    Arr,     // 71
    Arr,     // 72
    Arr,     // 73
    Arr,     // 74
    Foreign, // 75 AudioEvent
    Str,     // 76
    Str,     // 77
    Str,     // 78
    Str,     // 79
    Str,     // 80
    Str,     // 81
    Str,     // 82
    Foreign, // 83 CharacterSkin
    Foreign, // 84 OneHandSoundType
    Foreign, // 85 DropSoundEffect
    Foreign, // 86 Animation
    Str,     // 87 ShieldHeldAnimatedObject
    Str,     // 88 SheathAnimatedObject
];

pub const IVI_DDS_FILE: usize = 1;
```

Columns marked `array=True` in the community schema are `Arr` here regardless of their element type — in the fixed section an array is always 16 bytes. This is why entries 6–12 of `ItemVisualIdentity` are `Arr`, not `Str`.

- [ ] **Step 2: Write the failing tests**

Create `src-tauri/poe2-core/src/assets/dat.rs` with only this test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::schema::Col;

    const MAGIC: [u8; 8] = [0xBB; 8];

    /// Builds a .datc64 with schema [Str, I32, Foreign] and the given rows.
    /// Strings are written into the variable section as UTF-16LE with a
    /// four-zero-byte terminator, exactly as the game does.
    fn build(rows: &[(&str, i32, Option<u64>)]) -> Vec<u8> {
        let schema = [Col::Str, Col::I32, Col::Foreign];
        let width: usize = schema.iter().map(|c| c.width()).sum();
        assert_eq!(width, 28);

        let mut var: Vec<u8> = MAGIC.to_vec();
        let mut fixed = Vec::new();
        for (text, number, foreign) in rows {
            let str_offset = var.len() as u64; // relative to the magic's start
            for unit in text.encode_utf16() {
                var.extend(unit.to_le_bytes());
            }
            var.extend([0u8; 4]);

            fixed.extend(str_offset.to_le_bytes());
            fixed.extend(number.to_le_bytes());
            match foreign {
                Some(id) => {
                    fixed.extend(id.to_le_bytes());
                    fixed.extend(0u64.to_le_bytes());
                }
                None => fixed.extend([0xFEu8; 16]),
            }
        }

        let mut out = Vec::new();
        out.extend((rows.len() as u32).to_le_bytes());
        out.extend(fixed);
        out.extend(var);
        out
    }

    const SCHEMA: &[Col] = &[Col::Str, Col::I32, Col::Foreign];

    #[test]
    fn rows_and_strings_read_back() {
        let bytes = build(&[("Sapphire Ring", 7, Some(42)), ("Amber Amulet", -1, None)]);
        let dat = DatFile::parse(&bytes, SCHEMA).unwrap();
        assert_eq!(dat.row_count(), 2);
        assert_eq!(dat.string_at(0, 0, SCHEMA).as_deref(), Some("Sapphire Ring"));
        assert_eq!(dat.string_at(1, 0, SCHEMA).as_deref(), Some("Amber Amulet"));
    }

    #[test]
    fn foreign_rows_read_and_null_is_none() {
        let bytes = build(&[("A", 0, Some(42)), ("B", 0, None)]);
        let dat = DatFile::parse(&bytes, SCHEMA).unwrap();
        assert_eq!(dat.foreign_row(0, 2, SCHEMA), Some(42));
        assert_eq!(dat.foreign_row(1, 2, SCHEMA), None, "0xFE.. means null");
    }

    #[test]
    fn a_wrong_schema_is_refused_by_the_boundary_check() {
        // The file was built for 28-byte rows; a schema claiming different
        // widths puts the magic in the wrong place, and the file must refuse
        // to load rather than read garbage that looks like data. This is the
        // mechanism that turns a game patch into "icons missing" instead of
        // silent corruption.
        let bytes = build(&[("A", 0, Some(1)), ("B", 0, Some(2))]);
        let wrong: &[Col] = &[Col::Str, Col::I32, Col::Str]; // 20-byte rows
        match DatFile::parse(&bytes, wrong) {
            Err(crate::assets::AssetError::BadTable(_)) => {}
            other => panic!("expected BadTable, got {other:?}"),
        }
    }

    #[test]
    fn missing_magic_is_refused() {
        let mut bytes = build(&[("A", 0, None)]);
        for b in bytes.iter_mut().filter(|b| **b == 0xBB) {
            *b = 0xAA;
        }
        assert!(DatFile::parse(&bytes, SCHEMA).is_err());
    }

    #[test]
    fn out_of_range_row_or_column_yields_none_not_panic() {
        let bytes = build(&[("A", 0, None)]);
        let dat = DatFile::parse(&bytes, SCHEMA).unwrap();
        assert_eq!(dat.string_at(5, 0, SCHEMA), None);
        assert_eq!(dat.foreign_row(0, 99, SCHEMA), None);
        assert_eq!(dat.string_at(0, 1, SCHEMA), None, "column 1 is not a string");
    }

    #[test]
    fn a_string_offset_running_past_the_file_yields_none() {
        let mut bytes = build(&[("A", 0, None)]);
        // Corrupt the string offset to point far past the end.
        let huge = (u32::MAX as u64).to_le_bytes();
        bytes[4..12].copy_from_slice(&huge);
        let dat = DatFile::parse(&bytes, SCHEMA).unwrap();
        assert_eq!(dat.string_at(0, 0, SCHEMA), None);
    }

    #[test]
    fn the_vendored_schemas_have_the_measured_shapes() {
        use crate::assets::schema::*;
        assert_eq!(BASE_ITEM_TYPES.len(), 34);
        assert_eq!(ITEM_VISUAL_IDENTITY.len(), 89);
        assert_eq!(BASE_ITEM_TYPES[BIT_NAME], Col::Str);
        assert_eq!(BASE_ITEM_TYPES[BIT_VISUAL_IDENTITY], Col::Foreign);
        assert_eq!(ITEM_VISUAL_IDENTITY[IVI_DDS_FILE], Col::Str);
        // Offsets computed from the widths — the numbers the spike derived.
        let offset = |schema: &[Col], idx: usize| -> usize {
            schema[..idx].iter().map(|c| c.width()).sum()
        };
        assert_eq!(offset(BASE_ITEM_TYPES, BIT_NAME), 32);
        assert_eq!(offset(BASE_ITEM_TYPES, BIT_VISUAL_IDENTITY), 124);
        assert_eq!(offset(ITEM_VISUAL_IDENTITY, IVI_DDS_FILE), 8);
    }
}
```

- [ ] **Step 3: Run the tests to verify they fail**

```bash
cd src-tauri/poe2-core && cargo test assets::dat
```

Expected: compilation error — `cannot find type DatFile`.

- [ ] **Step 4: Write the implementation**

Insert above the `#[cfg(test)]` block in `src-tauri/poe2-core/src/assets/dat.rs`:

```rust
//! Reading .datc64 tables against a vendored schema.
//!
//! A .datc64 stores no column information of its own: it is `row_count: u32`,
//! fixed-width rows, then a variable-data section opening with eight 0xBB
//! bytes. What each column means lives in the vendored schema — and the one
//! integrity check available is that the magic must land exactly where the
//! schema says the rows end. When it does not, the game changed the table, and
//! refusing to load is the honest response.

use crate::assets::schema::Col;
use crate::assets::AssetError;

const MAGIC: [u8; 8] = [0xBB; 8];
const NULL_FOREIGN: u64 = 0xFEFE_FEFE_FEFE_FEFE;

pub struct DatFile {
    bytes: Vec<u8>,
    row_count: usize,
    row_width: usize,
    /// Offset of the magic — string offsets are relative to this.
    var_offset: usize,
}

impl DatFile {
    pub fn parse(bytes: &[u8], schema: &[Col]) -> Result<DatFile, AssetError> {
        if bytes.len() < 4 {
            return Err(AssetError::Truncated { need: 4, have: bytes.len() });
        }
        let row_count = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
        let row_width: usize = schema.iter().map(|c| c.width()).sum();
        let expected_magic_at = 4 + row_count * row_width;

        let magic_ok = bytes
            .get(expected_magic_at..expected_magic_at + 8)
            .map(|s| s == MAGIC)
            .unwrap_or(false);
        if !magic_ok {
            return Err(AssetError::BadTable(format!(
                "magic not at {expected_magic_at} for {row_count} rows of {row_width} bytes — \
                 the vendored schema no longer matches this table"
            )));
        }

        Ok(DatFile {
            bytes: bytes.to_vec(),
            row_count,
            row_width,
            var_offset: expected_magic_at,
        })
    }

    pub fn row_count(&self) -> usize {
        self.row_count
    }

    fn field_offset(&self, row: usize, col_index: usize, schema: &[Col]) -> Option<usize> {
        if row >= self.row_count || col_index >= schema.len() {
            return None;
        }
        let within: usize = schema[..col_index].iter().map(|c| c.width()).sum();
        Some(4 + row * self.row_width + within)
    }

    /// Reads a `Str` column: a u64 offset into the variable section, pointing
    /// at UTF-16LE text terminated by four zero bytes.
    pub fn string_at(&self, row: usize, col_index: usize, schema: &[Col]) -> Option<String> {
        if schema.get(col_index) != Some(&Col::Str) {
            return None;
        }
        let at = self.field_offset(row, col_index, schema)?;
        let rel = u64::from_le_bytes(self.bytes.get(at..at + 8)?.try_into().ok()?) as usize;
        let mut p = self.var_offset.checked_add(rel)?;
        let mut units = Vec::new();
        loop {
            let pair = self.bytes.get(p..p + 2)?;
            let unit = u16::from_le_bytes([pair[0], pair[1]]);
            if unit == 0 {
                // Terminator is four zero bytes; one zero unit is enough to stop.
                break;
            }
            units.push(unit);
            p += 2;
        }
        Some(String::from_utf16_lossy(&units))
    }

    /// Reads a `Foreign` column: the referenced row id, or None when null.
    pub fn foreign_row(&self, row: usize, col_index: usize, schema: &[Col]) -> Option<u64> {
        if schema.get(col_index) != Some(&Col::Foreign) {
            return None;
        }
        let at = self.field_offset(row, col_index, schema)?;
        let id = u64::from_le_bytes(self.bytes.get(at..at + 8)?.try_into().ok()?);
        (id != NULL_FOREIGN).then_some(id)
    }
}
```

Add `pub mod dat;` and `pub mod schema;` to `src-tauri/poe2-core/src/assets/mod.rs`.

- [ ] **Step 5: Run the tests to verify they pass**

```bash
cd src-tauri/poe2-core && cargo test
```

Expected: PASS — 7 new tests on top of the previous total.

- [ ] **Step 6: Format and commit**

```bash
cd src-tauri/poe2-core && cargo fmt
cd ../.. && git add src-tauri/poe2-core
git commit -m "feat(poe2): datc64 tables with vendored schemas and drift refusal"
```

---

## Task 4: The icon chain and the cache

**Files:**
- Create: `src-tauri/poe2-core/src/assets/icons.rs`
- Modify: `src-tauri/poe2-core/src/assets/mod.rs`, `src-tauri/poe2-core/Cargo.toml`

**Interfaces:**
- Consumes: everything from Tasks 1–3.
- Produces:
  - `assets::icons::CACHE_VERSION: &str` = `"1"`
  - `assets::icons::UI_ASSETS: &[(&str, &str)]` — pinned pairs of cache name → archive path; first entry `("inventory_square", "art/textures/interface/2d/2dart/uiimages/ingame/inventorysquare.dds")`. The doll project extends this list.
  - `assets::icons::TABLE_BASE_ITEM_TYPES: &str` = `"data/balance/baseitemtypes.datc64"`, `assets::icons::TABLE_ITEM_VISUAL_IDENTITY: &str` = `"data/balance/itemvisualidentity.datc64"`
  - `assets::icons::dds_to_png(dds_bytes: &[u8]) -> Result<Vec<u8>, AssetError>`
  - `assets::icons::IconCache::new(dir: &Path) -> Result<IconCache, AssetError>` — creates the directory, checks the version marker, wipes on mismatch; `path_for(&self, key: &str) -> PathBuf`; `get(&self, key: &str) -> Option<PathBuf>`; `put(&self, key: &str, png: &[u8]) -> Result<PathBuf, AssetError>`
  - `assets::icons::GameArchive::open(game_dir: &Path) -> Result<GameArchive, AssetError>` — loads the index once; `read(&self, path: &str) -> Result<Vec<u8>, AssetError>` decompresses the owning bundle and slices the file out
  - `assets::icons::IconSource::new(game_dir: &Path, cache_dir: &Path) -> IconSource` — lazy: opens nothing until first asked; `icon_for_base_type(&mut self, base_type: &str) -> Result<PathBuf, AssetError>`; `ui_asset(&mut self, name: &str) -> Result<PathBuf, AssetError>`

The base-type lookup tries the exact `Name` first, then falls back to the **longest table `Name` that is a suffix** of the requested base type. The player's own gear is why: bases like `Runeforged Tideseer Mantle` carry a prefix the table's plain name may not, and the suffix rule resolves them deterministically without guessing file names.

- [ ] **Step 1: Add the dependencies**

In `src-tauri/poe2-core/Cargo.toml`, append to `[dependencies]`:

```toml
image = { version = "0.25", default-features = false, features = ["png"] }
image_dds = { version = "0.7", default-features = false, features = ["ddsfile", "image"] }
```

The `image_dds` feature set is load-bearing: default features pull the encode path and with it Intel ISPC, which does not build on every machine.

- [ ] **Step 2: Write the failing tests**

Create `src-tauri/poe2-core/src/assets/icons.rs` with only this test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("poe2-icons-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        p
    }

    /// A minimal uncompressed RGBA DDS (DX10 header), 2x2 pixels — enough to
    /// prove the decode-and-encode path without hand-building BC blocks.
    fn tiny_dds() -> Vec<u8> {
        let mut b = Vec::new();
        b.extend(b"DDS ");
        let mut header = [0u8; 124];
        header[0..4].copy_from_slice(&124u32.to_le_bytes()); // size
        header[4..8].copy_from_slice(&0x0000_100Fu32.to_le_bytes()); // flags
        header[8..12].copy_from_slice(&2u32.to_le_bytes()); // height
        header[12..16].copy_from_slice(&2u32.to_le_bytes()); // width
        // pixel format block starts at offset 72 within the header
        header[72..76].copy_from_slice(&32u32.to_le_bytes()); // pf size
        header[76..80].copy_from_slice(&4u32.to_le_bytes()); // FOURCC flag
        header[80..84].copy_from_slice(b"DX10");
        b.extend_from_slice(&header);
        // DX10 extension: format 28 = R8G8B8A8_UNORM, dimension 3 = 2D
        b.extend(28u32.to_le_bytes());
        b.extend(3u32.to_le_bytes());
        b.extend(0u32.to_le_bytes());
        b.extend(1u32.to_le_bytes());
        b.extend(0u32.to_le_bytes());
        // 4 RGBA pixels
        b.extend([255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255]);
        b
    }

    #[test]
    fn a_dds_becomes_a_png() {
        let png = dds_to_png(&tiny_dds()).unwrap();
        assert_eq!(&png[1..4], b"PNG", "output must carry the PNG signature");
        let img = image::load_from_memory(&png).unwrap();
        assert_eq!((img.width(), img.height()), (2, 2));
    }

    #[test]
    fn garbage_is_a_bad_texture_not_a_panic() {
        match dds_to_png(b"not a dds at all") {
            Err(AssetError::BadTexture(_)) => {}
            other => panic!("expected BadTexture, got {other:?}"),
        }
    }

    #[test]
    fn the_cache_round_trips() {
        let dir = temp_dir("roundtrip");
        let cache = IconCache::new(&dir).unwrap();
        assert_eq!(cache.get("sapphire_ring"), None);
        let path = cache.put("sapphire_ring", b"pngbytes").unwrap();
        assert_eq!(cache.get("sapphire_ring").as_deref(), Some(path.as_path()));
        assert_eq!(std::fs::read(&path).unwrap(), b"pngbytes");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_version_mismatch_wipes_the_cache() {
        let dir = temp_dir("version");
        {
            let cache = IconCache::new(&dir).unwrap();
            cache.put("old", b"stale").unwrap();
        }
        std::fs::write(dir.join("version"), "0").unwrap();
        let cache = IconCache::new(&dir).unwrap();
        assert_eq!(
            cache.get("old"),
            None,
            "a cache written by an older scheme must not be served"
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("version")).unwrap(),
            CACHE_VERSION
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn cache_keys_cannot_escape_the_directory() {
        let dir = temp_dir("escape");
        let cache = IconCache::new(&dir).unwrap();
        let path = cache.path_for("../../evil");
        assert!(
            path.starts_with(&dir),
            "a hostile base-type name must stay inside the cache dir, got {path:?}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn suffix_matching_picks_the_longest_name() {
        let names = ["Mantle", "Tideseer Mantle", "Ring"];
        assert_eq!(
            best_name_match("Runeforged Tideseer Mantle", names.iter().copied()),
            Some("Tideseer Mantle")
        );
        assert_eq!(
            best_name_match("Sapphire Ring", ["Sapphire Ring", "Ring"].iter().copied()),
            Some("Sapphire Ring"),
            "an exact match wins outright"
        );
        assert_eq!(best_name_match("Unheard Of", names.iter().copied()), None);
    }

    #[test]
    fn suffix_matching_respects_word_boundaries() {
        // "opal ring" must not match a base named "pal ring".
        assert_eq!(
            best_name_match("Opal Ring", ["pal Ring"].iter().copied()),
            None,
            "a suffix must start at a word boundary"
        );
    }
}
```

- [ ] **Step 3: Run the tests to verify they fail**

```bash
cd src-tauri/poe2-core && cargo test assets::icons
```

Expected: compilation error — `cannot find function dds_to_png`.

- [ ] **Step 4: Write the implementation**

Insert above the `#[cfg(test)]` block in `src-tauri/poe2-core/src/assets/icons.rs`:

```rust
//! From a captured item's base type to a PNG on disk.
//!
//! Every step of the chain can fail independently, and each failure is one
//! missing icon — never a broken caller. The archives open lazily and at most
//! once per `IconSource`; after that the cache answers from disk.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::assets::bundles::decompress;
use crate::assets::dat::DatFile;
use crate::assets::index::BundleIndex;
use crate::assets::schema::{
    BASE_ITEM_TYPES, BIT_NAME, BIT_VISUAL_IDENTITY, ITEM_VISUAL_IDENTITY, IVI_DDS_FILE,
};
use crate::assets::AssetError;

/// Bump when the extraction scheme changes; an old cache is wiped, not served.
pub const CACHE_VERSION: &str = "1";

pub const TABLE_BASE_ITEM_TYPES: &str = "data/balance/baseitemtypes.datc64";
pub const TABLE_ITEM_VISUAL_IDENTITY: &str = "data/balance/itemvisualidentity.datc64";

/// Interface chrome, pinned by the spike. The doll project extends this list.
pub const UI_ASSETS: &[(&str, &str)] = &[(
    "inventory_square",
    "art/textures/interface/2d/2dart/uiimages/ingame/inventorysquare.dds",
)];

/// Decodes any DDS the game ships (BC-compressed or raw) and encodes PNG.
pub fn dds_to_png(dds_bytes: &[u8]) -> Result<Vec<u8>, AssetError> {
    let dds = ddsfile_read(dds_bytes)?;
    let img = image_dds::image_from_dds(&dds, 0)
        .map_err(|e| AssetError::BadTexture(format!("decode: {e}")))?;
    let mut png = Vec::new();
    image::DynamicImage::ImageRgba8(img)
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .map_err(|e| AssetError::BadTexture(format!("png encode: {e}")))?;
    Ok(png)
}

fn ddsfile_read(bytes: &[u8]) -> Result<image_dds::ddsfile::Dds, AssetError> {
    image_dds::ddsfile::Dds::read(&mut std::io::Cursor::new(bytes))
        .map_err(|e| AssetError::BadTexture(format!("dds header: {e}")))
}

/// The exact table name if present; otherwise the longest table name that is a
/// whole-word suffix of the base type. Bases like "Runeforged Tideseer Mantle"
/// carry a crafted prefix the table's plain name lacks.
pub fn best_name_match<'a>(
    base_type: &str,
    names: impl Iterator<Item = &'a str>,
) -> Option<&'a str> {
    let mut best: Option<&str> = None;
    for name in names {
        if name == base_type {
            return Some(name);
        }
        let is_suffix = base_type.ends_with(name)
            && base_type[..base_type.len() - name.len()].ends_with(' ');
        if is_suffix && best.map(|b| name.len() > b.len()).unwrap_or(true) {
            best = Some(name);
        }
    }
    best
}

pub struct IconCache {
    dir: PathBuf,
}

impl IconCache {
    pub fn new(dir: &Path) -> Result<IconCache, AssetError> {
        std::fs::create_dir_all(dir).map_err(|e| AssetError::Io(e.to_string()))?;
        let version_file = dir.join("version");
        let current = std::fs::read_to_string(&version_file).unwrap_or_default();
        if current != CACHE_VERSION {
            // Wipe and restamp: serving icons produced by an older scheme is
            // exactly the stale-picture bug the marker exists to prevent.
            for entry in std::fs::read_dir(dir).map_err(|e| AssetError::Io(e.to_string()))? {
                let entry = entry.map_err(|e| AssetError::Io(e.to_string()))?;
                let _ = std::fs::remove_file(entry.path());
            }
            std::fs::write(&version_file, CACHE_VERSION)
                .map_err(|e| AssetError::Io(e.to_string()))?;
        }
        Ok(IconCache { dir: dir.to_path_buf() })
    }

    /// A key becomes a flat file name; anything path-like is flattened so a
    /// hostile base-type string cannot climb out of the cache directory.
    pub fn path_for(&self, key: &str) -> PathBuf {
        let safe: String = key
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
            .collect();
        self.dir.join(format!("{safe}.png"))
    }

    pub fn get(&self, key: &str) -> Option<PathBuf> {
        let path = self.path_for(key);
        path.exists().then_some(path)
    }

    pub fn put(&self, key: &str, png: &[u8]) -> Result<PathBuf, AssetError> {
        let path = self.path_for(key);
        std::fs::write(&path, png).map_err(|e| AssetError::Io(e.to_string()))?;
        Ok(path)
    }
}

/// The opened archives: index plus per-bundle decompression with a small memo.
pub struct GameArchive {
    root: PathBuf,
    index: BundleIndex,
    bundle_memo: HashMap<String, Vec<u8>>,
}

impl GameArchive {
    pub fn open(game_dir: &Path) -> Result<GameArchive, AssetError> {
        let index_path = game_dir.join("Bundles2").join("_.index.bin");
        let bytes = std::fs::read(&index_path)
            .map_err(|e| AssetError::Io(format!("{}: {e}", index_path.display())))?;
        Ok(GameArchive {
            root: game_dir.to_path_buf(),
            index: BundleIndex::load(&bytes)?,
            bundle_memo: HashMap::new(),
        })
    }

    pub fn index(&self) -> &BundleIndex {
        &self.index
    }

    pub fn read(&mut self, path: &str) -> Result<Vec<u8>, AssetError> {
        let loc = self
            .index
            .find(path)
            .ok_or_else(|| AssetError::NotFound(path.to_string()))?;
        if !self.bundle_memo.contains_key(&loc.bundle) {
            let bundle_path = self
                .root
                .join("Bundles2")
                .join(format!("{}.bundle.bin", loc.bundle.replace('/', std::path::MAIN_SEPARATOR_STR)));
            let raw = std::fs::read(&bundle_path)
                .map_err(|e| AssetError::Io(format!("{}: {e}", bundle_path.display())))?;
            self.bundle_memo.insert(loc.bundle.clone(), decompress(&raw)?);
        }
        let content = &self.bundle_memo[&loc.bundle];
        content
            .get(loc.offset..loc.offset + loc.size)
            .map(|s| s.to_vec())
            .ok_or(AssetError::Truncated {
                need: loc.offset + loc.size,
                have: content.len(),
            })
    }
}

/// Lazily-opened source of icons. Archives and tables load on first use only;
/// everything already cached is served without touching the game at all.
pub struct IconSource {
    game_dir: PathBuf,
    cache: Result<IconCache, AssetError>,
    archive: Option<GameArchive>,
    /// base-type Name -> ItemVisualIdentity row id
    name_to_ivi: Option<HashMap<String, u64>>,
    /// ItemVisualIdentity row id -> DDS path
    ivi_to_dds: Option<HashMap<u64, String>>,
}

impl IconSource {
    pub fn new(game_dir: &Path, cache_dir: &Path) -> IconSource {
        IconSource {
            game_dir: game_dir.to_path_buf(),
            cache: IconCache::new(cache_dir),
            archive: None,
            name_to_ivi: None,
            ivi_to_dds: None,
        }
    }

    fn cache(&self) -> Result<&IconCache, AssetError> {
        self.cache
            .as_ref()
            .map_err(|e| AssetError::Io(e.to_string()))
    }

    fn archive(&mut self) -> Result<&mut GameArchive, AssetError> {
        if self.archive.is_none() {
            self.archive = Some(GameArchive::open(&self.game_dir)?);
        }
        Ok(self.archive.as_mut().unwrap())
    }

    fn ensure_tables(&mut self) -> Result<(), AssetError> {
        if self.name_to_ivi.is_some() {
            return Ok(());
        }
        let bit_bytes = self.archive()?.read(TABLE_BASE_ITEM_TYPES)?;
        let ivi_bytes = self.archive()?.read(TABLE_ITEM_VISUAL_IDENTITY)?;
        let bit = DatFile::parse(&bit_bytes, BASE_ITEM_TYPES)?;
        let ivi = DatFile::parse(&ivi_bytes, ITEM_VISUAL_IDENTITY)?;

        let mut names = HashMap::with_capacity(bit.row_count());
        for row in 0..bit.row_count() {
            if let (Some(name), Some(id)) = (
                bit.string_at(row, BIT_NAME, BASE_ITEM_TYPES),
                bit.foreign_row(row, BIT_VISUAL_IDENTITY, BASE_ITEM_TYPES),
            ) {
                if !name.is_empty() {
                    names.insert(name, id);
                }
            }
        }
        let mut dds = HashMap::with_capacity(ivi.row_count());
        for row in 0..ivi.row_count() {
            if let Some(path) = ivi.string_at(row, IVI_DDS_FILE, ITEM_VISUAL_IDENTITY) {
                if !path.is_empty() {
                    dds.insert(row as u64, path);
                }
            }
        }
        self.name_to_ivi = Some(names);
        self.ivi_to_dds = Some(dds);
        Ok(())
    }

    pub fn icon_for_base_type(&mut self, base_type: &str) -> Result<PathBuf, AssetError> {
        if let Some(hit) = self.cache()?.get(base_type) {
            return Ok(hit);
        }
        self.ensure_tables()?;
        let names = self.name_to_ivi.as_ref().unwrap();
        let matched = best_name_match(base_type, names.keys().map(String::as_str))
            .ok_or_else(|| AssetError::NotFound(format!("base type: {base_type}")))?;
        let ivi_row = names[matched];
        let dds_path = self
            .ivi_to_dds
            .as_ref()
            .unwrap()
            .get(&ivi_row)
            .cloned()
            .ok_or_else(|| {
                AssetError::NotFound(format!("visual identity row {ivi_row} for {base_type}"))
            })?;
        // Archive paths are stored lowercased in the decoded list.
        let dds_bytes = self.archive()?.read(&dds_path.to_lowercase())?;
        let png = dds_to_png(&dds_bytes)?;
        self.cache()?.put(base_type, &png)
    }

    pub fn ui_asset(&mut self, name: &str) -> Result<PathBuf, AssetError> {
        let key = format!("ui_{name}");
        if let Some(hit) = self.cache()?.get(&key) {
            return Ok(hit);
        }
        let path = UI_ASSETS
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, p)| *p)
            .ok_or_else(|| AssetError::NotFound(format!("ui asset: {name}")))?;
        let dds_bytes = self.archive()?.read(path)?;
        let png = dds_to_png(&dds_bytes)?;
        self.cache()?.put(&key, &png)
    }
}
```

Add `pub mod icons;` to `src-tauri/poe2-core/src/assets/mod.rs`.

- [ ] **Step 5: Run the tests to verify they pass**

```bash
cd src-tauri/poe2-core && cargo test
```

Expected: PASS — 8 new tests on top of the previous total. The first build takes a few minutes: `image` and `image_dds` compile once.

- [ ] **Step 6: Format and commit**

```bash
cd src-tauri/poe2-core && cargo fmt
cd ../.. && git add src-tauri/poe2-core
git commit -m "feat(poe2): the icon chain from base type to cached PNG"
```

---

## Task 5: Commands and the background fill

**Files:**
- Create: `src-tauri/src/poe2/assets_task.rs`
- Modify: `src-tauri/src/poe2/mod.rs`, `src-tauri/src/poe2/commands.rs`, `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `poe2_core::assets::icons::{IconSource, UI_ASSETS}`; `store_for` and `tracker::log_path` from the existing Tauri code.
- Produces:
  - `assets_task::game_dir(app: &AppHandle) -> PathBuf` — two levels up from the configured `Client.txt`
  - `assets_task::icon_cache_dir(app: &AppHandle) -> Result<PathBuf, String>` — `poe2-icons` beside `poe2.db`
  - `assets_task::spawn_fill(app: AppHandle)` — a background thread that extracts icons for every captured base type plus every `UI_ASSETS` entry, then emits `poe2://icons-changed` (no payload) if anything new landed; guarded by an `AtomicBool` with `compare_exchange`, and the guard **clears when the pass finishes** so a later capture can trigger another pass
  - command `poe2_item_icon(app: AppHandle, base_type: String) -> Result<Option<String>, String>` — the cached PNG's absolute path, `Ok(None)` when unavailable; **never extracts inline** (a UI call must not block on a 114 MB index), it only reads the cache
  - command `poe2_ui_asset(app: AppHandle, name: String) -> Result<Option<String>, String>` — same contract

- [ ] **Step 1: Write the task module**

Create `src-tauri/src/poe2/assets_task.rs`:

```rust
//! Filling the icon cache in the background.
//!
//! Opening the archives costs seconds and hundreds of megabytes of transient
//! memory — the index alone is 114 MB compressed. That work happens on this
//! thread, once per pass; the `poe2_item_icon` command itself only ever reads
//! the cache, so the UI never waits on an extraction.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter};

use poe2_core::assets::icons::{IconSource, UI_ASSETS};

use crate::poe2::commands::store_for;
use crate::settings;

pub const ICONS_CHANGED_EVENT: &str = "poe2://icons-changed";

static FILL_RUNNING: AtomicBool = AtomicBool::new(false);

/// The game root: `Client.txt` lives in `<game>/logs/`, so two levels up.
pub fn game_dir(app: &AppHandle) -> PathBuf {
    let log = crate::poe2::tracker::log_path(app);
    log.parent()
        .and_then(|p| p.parent())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn icon_cache_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = crate::portable::app_data_dir(app).map_err(|e| e.to_string())?;
    Ok(dir.join("poe2-icons"))
}

/// One background pass over everything we might need. Safe to call repeatedly:
/// a second call while a pass runs is a no-op, and the flag clears when the
/// pass ends so a later capture can start a new one.
pub fn spawn_fill(app: AppHandle) {
    if !settings::get_settings(&app).poe2_enabled {
        return;
    }
    if FILL_RUNNING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }

    std::thread::spawn(move || {
        let result = fill_once(&app);
        FILL_RUNNING.store(false, Ordering::SeqCst);
        match result {
            Ok(true) => {
                let _ = app.emit(ICONS_CHANGED_EVENT, ());
            }
            Ok(false) => {}
            Err(e) => log::warn!("poe2 icon fill: {e}"),
        }
    });
}

/// Returns whether anything new was extracted.
fn fill_once(app: &AppHandle) -> Result<bool, String> {
    let cache_dir = icon_cache_dir(app)?;
    let mut source = IconSource::new(&game_dir(app), &cache_dir);

    let store = store_for(app)?;
    let items = store.all_items().map_err(|e| e.to_string())?;

    let mut base_types: Vec<String> = items
        .iter()
        .filter_map(|i| i.base_type.clone())
        .collect();
    base_types.sort();
    base_types.dedup();

    let mut changed = false;
    for (name, _) in UI_ASSETS {
        match source.ui_asset(name) {
            Ok(_) => changed = true,
            // Never the raw path of the player's files in the log — names only.
            Err(e) => log::warn!("poe2 icon fill: ui asset {name}: {e}"),
        }
    }
    for base in &base_types {
        match source.icon_for_base_type(base) {
            Ok(_) => changed = true,
            Err(e) => log::warn!("poe2 icon fill: {base}: {e}"),
        }
    }
    Ok(changed)
}
```

`changed` flips on every successful call including cache hits — acceptable: the event fires at most once per pass, and a spurious refresh costs one cache read. Note it in the report; do not add bookkeeping to avoid it.

- [ ] **Step 2: Add the commands**

In `src-tauri/src/poe2/commands.rs`, add:

```rust
#[tauri::command]
#[specta::specta]
pub fn poe2_item_icon(app: AppHandle, base_type: String) -> Result<Option<String>, String> {
    let cache_dir = crate::poe2::assets_task::icon_cache_dir(&app)?;
    let cache = poe2_core::assets::icons::IconCache::new(&cache_dir).map_err(|e| e.to_string())?;
    Ok(cache
        .get(&base_type)
        .map(|p| p.to_string_lossy().into_owned()))
}

#[tauri::command]
#[specta::specta]
pub fn poe2_ui_asset(app: AppHandle, name: String) -> Result<Option<String>, String> {
    let cache_dir = crate::poe2::assets_task::icon_cache_dir(&app)?;
    let cache = poe2_core::assets::icons::IconCache::new(&cache_dir).map_err(|e| e.to_string())?;
    Ok(cache
        .get(&format!("ui_{name}"))
        .map(|p| p.to_string_lossy().into_owned()))
}
```

- [ ] **Step 3: Wire it up**

- `pub mod assets_task;` in `src-tauri/src/poe2/mod.rs`.
- In `src-tauri/src/lib.rs`: add `poe2::commands::poe2_item_icon` and `poe2::commands::poe2_ui_asset` to `collect_commands![…]`, and `crate::poe2::assets_task::spawn_fill(app.handle().clone());` in the `.setup(...)` closure beside the existing tracker and watcher spawns.
- In the clipboard watcher's stored-item branch (`src-tauri/src/poe2/watcher.rs` is **not** in the editable list — so instead): in `change_poe2_enabled_setting` in `commands.rs`, alongside the existing spawns, add `crate::poe2::assets_task::spawn_fill(app.clone());` so enabling the section also fills icons. A fill for items captured mid-session arrives on the next enable or restart; the doll project may add a finer trigger if it needs one.

- [ ] **Step 4: Verify**

```bash
cd src-tauri/poe2-core && cargo test
cd .. && cargo build
```

Expected: core suite unchanged; the `handy` crate compiles. If the build fails because a running `handy.exe` holds a DLL, stop that process and say so.

- [ ] **Step 5: Format and commit**

```bash
cd src-tauri && cargo fmt
cd .. && git status --short
```

Revert unrelated drift, then:

```bash
git add src-tauri/src
git commit -m "feat(poe2): icon commands and background cache fill"
```

---

## Task 6: Acceptance against the real game

**Files:**
- Create: `src-tauri/poe2-core/src/assets/acceptance.rs`
- Modify: `src-tauri/poe2-core/src/assets/mod.rs`

**Interfaces:**
- Consumes: everything from Tasks 1–4.
- Produces: nothing new.

Every test here is `#[ignore]`d and begins with the same guard: if the game directory does not exist, print why and return. They run only by explicit request on this machine, exactly like the tracker's `real_log` test.

- [ ] **Step 1: Write the acceptance tests**

Create `src-tauri/poe2-core/src/assets/acceptance.rs`:

```rust
//! Acceptance tests for the definition of done in
//! docs/superpowers/specs/2026-08-06-poe2-game-assets-design.md.
//!
//! Criteria that cannot be tested here and where they are enforced instead:
//!   - Criterion 4 (the Equipment tab keeps working without the game) is the
//!     calculator's own guarantee: nothing in gear/ or the equipment command
//!     calls into assets/. Enforced by the absence of such a call — grep for
//!     `assets::` under gear/ finds nothing.
//!   - Criterion 5's build half (a schema break never fails the build) holds
//!     because schemas are data compared at runtime; the boundary check turns
//!     drift into AssetError::BadTable, covered by unit tests in dat.rs.
//!   - Criterion 7 (no network) is enforced by dependency review: none of the
//!     three crates performs I/O beyond the file system.

#[cfg(test)]
mod tests {
    use crate::assets::icons::{GameArchive, IconSource, UI_ASSETS};
    use std::path::{Path, PathBuf};

    const GAME_DIR: &str = r"C:\Program Files (x86)\Steam\steamapps\common\Path of Exile 2";

    /// The player's nine captured base types, from the live database.
    const BASE_TYPES: [&str; 9] = [
        "Rattling Sceptre",
        "Runeforged Tideseer Mantle",
        "Runeforged Jade Tiara",
        "Runeforged Crystal Focus",
        "Runeforged Elegant Slippers",
        "Runeforged Goldcast Cuffs",
        "Sapphire Ring",
        "Amethyst Ring",
        "Amber Amulet",
    ];

    fn game() -> Option<PathBuf> {
        let p = Path::new(GAME_DIR);
        if p.join("Bundles2").join("_.index.bin").exists() {
            Some(p.to_path_buf())
        } else {
            println!("skipped: game not found at {GAME_DIR}");
            None
        }
    }

    fn temp_cache(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("poe2-acceptance-icons-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        p
    }

    /// Criterion 1: an icon for each of the nine items, or a named reason.
    #[test]
    #[ignore]
    fn every_captured_item_yields_an_icon_or_a_reason() {
        let Some(game) = game() else { return };
        let cache = temp_cache("nine");
        let mut source = IconSource::new(&game, &cache);

        let mut missing = Vec::new();
        for base in BASE_TYPES {
            match source.icon_for_base_type(base) {
                Ok(path) => {
                    let bytes = std::fs::read(&path).unwrap();
                    assert_eq!(&bytes[1..4], b"PNG", "{base}: not a png");
                    println!("ok: {base} -> {} ({} bytes)", path.display(), bytes.len());
                }
                Err(e) => {
                    println!("MISSING: {base}: {e}");
                    missing.push(base);
                }
            }
        }
        // The suffix rule exists precisely for the Runeforged bases; the plain
        // bases must resolve outright. Allow no more than two misses total so
        // a schema drift screams while a single odd base does not.
        assert!(
            missing.len() <= 2,
            "too many unresolved base types: {missing:?}"
        );
        std::fs::remove_dir_all(&cache).ok();
    }

    /// Criterion 2: the inventory chrome extracts.
    #[test]
    #[ignore]
    fn the_slot_frame_extracts() {
        let Some(game) = game() else { return };
        let cache = temp_cache("chrome");
        let mut source = IconSource::new(&game, &cache);
        for (name, _) in UI_ASSETS {
            let path = source.ui_asset(name).unwrap();
            let img = image::load_from_memory(&std::fs::read(&path).unwrap()).unwrap();
            assert!(img.width() > 0 && img.height() > 0);
            println!("ok: {name} {}x{}", img.width(), img.height());
        }
        std::fs::remove_dir_all(&cache).ok();
    }

    /// Criterion 3: the second request never touches the archives.
    #[test]
    #[ignore]
    fn the_cache_answers_without_the_game() {
        let Some(game) = game() else { return };
        let cache = temp_cache("cached");
        {
            let mut source = IconSource::new(&game, &cache);
            source.icon_for_base_type("Sapphire Ring").unwrap();
        }
        // A source pointed at a directory with no game can only answer from
        // the cache — which is the claim.
        let bogus = Path::new(r"C:\does\not\exist");
        let mut cold = IconSource::new(bogus, &cache);
        let path = cold.icon_for_base_type("Sapphire Ring").unwrap();
        assert!(path.exists());
        std::fs::remove_dir_all(&cache).ok();
    }

    /// The measured index shape still holds — an early tripwire for patches.
    #[test]
    #[ignore]
    fn the_index_still_parses_and_paths_still_match_files() {
        let Some(game) = game() else { return };
        let mut archive = GameArchive::open(&game).unwrap();
        let tables = archive
            .index()
            .paths_with_prefix("data/balance/baseitemtypes")
            .len();
        assert!(tables >= 1, "the base item table vanished from the index");
        let bytes = archive.read("data/balance/baseitemtypes.datc64").unwrap();
        assert!(!bytes.is_empty());
    }
}
```

Add `#[cfg(test)] mod acceptance;` to `src-tauri/poe2-core/src/assets/mod.rs`.

- [ ] **Step 2: Run the ordinary suite, then the ignored tests**

```bash
cd src-tauri/poe2-core && cargo test
cd src-tauri/poe2-core && cargo test assets::acceptance -- --ignored --nocapture
```

Expected: the ordinary suite green and unchanged in count; then all four ignored tests pass against the real game, printing each icon's path and size. **Report the printed output verbatim** — the resolved-or-missing list for the nine base types is the deliverable of this whole project. If more than two base types miss, stop and report rather than loosening the assertion.

- [ ] **Step 3: The full gates**

```bash
cd src-tauri && cargo build
cd src-tauri/poe2-core && cargo fmt -- --check
cd .. && bun run lint
bunx tsc --noEmit
```

All clean (the `src-tauri`-wide `fmt --check` still fails on the fork's pre-existing drift — report as pre-existing).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/poe2-core
git commit -m "test(poe2): acceptance for game asset extraction against the real install"
```

---

## Checking the plan against the spec's definition of done

| Criterion from the spec | Where it is met |
|---|---|
| 1. An icon per captured item, or a named reason | Task 4 chain; Task 6, `every_captured_item_yields_an_icon_or_a_reason` |
| 2. Slot frames and doll background extracted from the game's UI art | Task 4, `UI_ASSETS` (pinned `inventorysquare.dds`, list extendable by the doll project); Task 6, `the_slot_frame_extracts` |
| 3. Second request served from cache without opening any archive | Task 4, lazy `IconSource` + cache-first lookup; Task 6, `the_cache_answers_without_the_game` |
| 4. Without the game, the Equipment tab still works | Nothing in `gear/` or the equipment command calls `assets::` — noted in the acceptance doc comment; commands return `Ok(None)` on a cold cache |
| 5. A schema patch degrades to missing icons, never a broken tab or build | Task 3, the boundary check and `a_wrong_schema_is_refused_by_the_boundary_check`; schemas are runtime data |
| 6. `image_dds` decode-only, clean build from checkout | Task 4 step 1 (pinned features); Task 6 step 3 (`cargo build`) |
| 7. No network anywhere | Vendored schemas (Task 3); dependency set has no network capability — noted in the acceptance doc comment |
| Automatic extraction, no button | Task 5, `spawn_fill` at startup and on section enable |
| Icons found via the tables, not name guessing | Task 4, `ensure_tables` + `best_name_match`; justified by the measured absence of `sapphirering.dds` |
| Unknown encoder / truncated data fail loudly, never guess | Task 1, `an_unknown_encoder_is_refused_by_decompress` and truncation tests |
| A hostile base-type string cannot escape the cache dir | Task 4, `cache_keys_cannot_escape_the_directory` |
| Player-file paths never logged raw | Task 5, the fill logs names and reasons only |
