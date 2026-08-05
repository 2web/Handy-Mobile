//! Storage for captured items.
//!
//! Raw text is the source of truth and structure is derived: the parser will get
//! smarter, and stored items must be able to reparse themselves without the
//! player pasting anything again.
//!
//! A separate database file from Handy's own: game data has nothing to do with
//! dictation history, and a separate file can be deleted and rebuilt without
//! touching anything else.

use anyhow::Result;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use rusqlite_migration::{Migrations, M};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use specta::Type;
use std::collections::BTreeMap;
use std::path::Path;

use crate::items::ParsedItem;

const MIGRATIONS: [&str; 2] = [
    "CREATE TABLE IF NOT EXISTS items (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        raw_hash TEXT NOT NULL UNIQUE,
        captured_ts TEXT NOT NULL,
        raw_text TEXT NOT NULL,
        source TEXT NOT NULL,
        item_class TEXT,
        rarity TEXT,
        name TEXT,
        base_type TEXT,
        item_level INTEGER,
        requires_level INTEGER,
        quality INTEGER,
        sockets TEXT,
        properties TEXT NOT NULL DEFAULT '{}',
        requirements TEXT NOT NULL DEFAULT '{}',
        advanced INTEGER NOT NULL DEFAULT 0
    );",
    "CREATE TABLE IF NOT EXISTS item_mods (
        item_id INTEGER NOT NULL,
        position INTEGER NOT NULL,
        effect_index INTEGER NOT NULL,
        kind TEXT NOT NULL,
        mod_name TEXT,
        tier INTEGER,
        tags TEXT NOT NULL DEFAULT '[]',
        text TEXT NOT NULL,
        value REAL,
        value_min REAL,
        value_max REAL,
        PRIMARY KEY (item_id, position, effect_index)
    );",
];

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct StoredMod {
    pub position: i64,
    pub effect_index: i64,
    pub kind: String,
    pub mod_name: Option<String>,
    pub tier: Option<i64>,
    pub tags: Vec<String>,
    pub text: String,
    pub value: Option<f64>,
    pub value_min: Option<f64>,
    pub value_max: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct StoredItem {
    pub id: i64,
    pub captured_ts: String,
    pub raw_text: String,
    pub source: String,
    pub item_class: Option<String>,
    pub rarity: Option<String>,
    pub name: Option<String>,
    pub base_type: Option<String>,
    pub item_level: Option<i64>,
    pub requires_level: Option<i64>,
    pub quality: Option<i64>,
    pub sockets: Option<String>,
    pub properties: BTreeMap<String, String>,
    pub requirements: BTreeMap<String, i64>,
    pub advanced: bool,
    pub mods: Vec<StoredMod>,
}

pub struct Poe2Store {
    conn: Connection,
}

impl Poe2Store {
    pub fn open(path: &Path) -> Result<Poe2Store> {
        Self::from_connection(Connection::open(path)?)
    }

    /// In-memory store for tests. The connection is held for the lifetime of the
    /// struct: an in-memory database vanishes the moment its last connection
    /// closes, so this must not reopen per call.
    pub fn in_memory() -> Result<Poe2Store> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(mut conn: Connection) -> Result<Poe2Store> {
        // A background clipboard watcher and a command handler can both open a
        // connection to this file and write at the same time. Without a busy
        // timeout, the connection that loses the race to SQLite's write lock
        // fails immediately with SQLITE_BUSY instead of waiting — turning a
        // momentary overlap into a lost item. Five seconds is far longer than
        // any write here takes.
        conn.busy_timeout(std::time::Duration::from_secs(5))?;

        let migrations = Migrations::new(MIGRATIONS.iter().map(|sql| M::up(sql)).collect());
        migrations.to_latest(&mut conn)?;
        Ok(Poe2Store { conn })
    }

    /// Stores an item. Returns (id, whether a new record was created).
    ///
    /// The duplicate key is a hash of the raw text: the same item pasted twice
    /// does not create a second record.
    ///
    /// The item row and its modifier rows commit together inside one
    /// transaction: a reader must never be able to observe an item whose
    /// modifiers are partially written, because later work sums modifiers into
    /// totals that a truncated item would silently get wrong.
    pub fn add_item(
        &mut self,
        parsed: &ParsedItem,
        source: &str,
        captured_ts: DateTime<Utc>,
    ) -> Result<(i64, bool)> {
        let raw_hash = format!("{:x}", Sha256::digest(parsed.raw_text.as_bytes()));

        // The duplicate check runs inside the same transaction as the insert,
        // not before it: a background capture and a manual paste of the same
        // item can otherwise both read "no existing row" before either writes,
        // and the loser then trips the `raw_hash` UNIQUE constraint instead of
        // returning the existing id cleanly. `Immediate` takes SQLite's write
        // lock at `BEGIN` rather than at the first write statement, so a
        // second connection's transaction blocks (subject to `busy_timeout`)
        // for the whole duration of this one instead of racing its own SELECT
        // against this transaction's not-yet-committed INSERT.
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing: Option<i64> = tx
            .query_row(
                "SELECT id FROM items WHERE raw_hash = ?1",
                params![raw_hash],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(id) = existing {
            return Ok((id, false));
        }

        tx.execute(
            "INSERT INTO items (raw_hash, captured_ts, raw_text, source, item_class, rarity,
                name, base_type, item_level, requires_level, quality, sockets, properties,
                requirements, advanced)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                raw_hash,
                captured_ts.to_rfc3339(),
                parsed.raw_text,
                source,
                parsed.item_class,
                parsed.rarity,
                parsed.name,
                parsed.base_type,
                parsed.item_level,
                parsed.requires_level,
                parsed.quality,
                parsed.sockets,
                serde_json::to_string(&parsed.properties)?,
                serde_json::to_string(&parsed.requirements)?,
                parsed.advanced as i64,
            ],
        )?;
        let id = tx.last_insert_rowid();
        Self::insert_mods(&tx, id, parsed)?;
        tx.commit()?;
        Ok((id, true))
    }

    /// A row is one effect. Effects of the same modifier share a position.
    ///
    /// That is how the format's central property survives a flat table: a prefix
    /// may produce several lines and still be one modifier.
    ///
    /// Takes a `&Connection` rather than `&self` so it can run against either
    /// the store's own connection or a `Transaction` (which derefs to one) —
    /// callers decide the transaction boundary, this just does the inserts.
    fn insert_mods(conn: &Connection, item_id: i64, parsed: &ParsedItem) -> Result<()> {
        let mut stmt = conn.prepare(
            "INSERT INTO item_mods (item_id, position, effect_index, kind, mod_name, tier,
                tags, text, value, value_min, value_max)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        )?;

        for (position, item_mod) in parsed.mods.iter().enumerate() {
            let tags = serde_json::to_string(&item_mod.tags)?;
            for (effect_index, effect) in item_mod.effects.iter().enumerate() {
                // The second value of an effect ("from and to") does not fit the
                // columns and is recoverable by reparsing. The first consumer —
                // summing resistances — never needs it: a resistance is one number.
                let first = effect.values.first();
                stmt.execute(params![
                    item_id,
                    position as i64,
                    effect_index as i64,
                    item_mod.kind.as_str(),
                    item_mod.name,
                    item_mod.tier,
                    tags,
                    effect.text,
                    first.map(|v| v.value),
                    first.and_then(|v| v.value_min),
                    first.and_then(|v| v.value_max),
                ])?;
            }
        }
        Ok(())
    }

    /// Replaces the parse of an item without touching raw text or capture time.
    ///
    /// The update, the delete of the old modifier rows, and the insert of the
    /// new ones all happen inside one transaction. Without that, a failure
    /// between the delete and the re-insert would leave an item with zero
    /// modifiers that still reads back as a normal, complete `StoredItem`.
    pub fn reparse_item(&mut self, id: i64, parsed: &ParsedItem) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "UPDATE items SET item_class = ?1, rarity = ?2, name = ?3, base_type = ?4,
                item_level = ?5, requires_level = ?6, quality = ?7, sockets = ?8,
                properties = ?9, requirements = ?10, advanced = ?11 WHERE id = ?12",
            params![
                parsed.item_class,
                parsed.rarity,
                parsed.name,
                parsed.base_type,
                parsed.item_level,
                parsed.requires_level,
                parsed.quality,
                parsed.sockets,
                serde_json::to_string(&parsed.properties)?,
                serde_json::to_string(&parsed.requirements)?,
                parsed.advanced as i64,
                id,
            ],
        )?;
        tx.execute("DELETE FROM item_mods WHERE item_id = ?1", params![id])?;
        Self::insert_mods(&tx, id, parsed)?;
        tx.commit()?;
        Ok(())
    }

    /// (id, raw text) pairs for reparsing. Raw text is the source of truth.
    pub fn raw_items(&self) -> Result<Vec<(i64, String)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, raw_text FROM items ORDER BY id")?;
        let rows = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<Vec<(i64, String)>>>()?;
        Ok(rows)
    }

    pub fn items(&self, limit: i64) -> Result<Vec<StoredItem>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM items ORDER BY id DESC LIMIT ?1")?;
        let ids = stmt
            .query_map(params![limit], |row| row.get::<_, i64>(0))?
            .collect::<rusqlite::Result<Vec<i64>>>()?;

        let mut items = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(item) = self.item(id)? {
                items.push(item);
            }
        }
        Ok(items)
    }

    pub fn item(&self, id: i64) -> Result<Option<StoredItem>> {
        let row = self
            .conn
            .query_row(
                "SELECT id, captured_ts, raw_text, source, item_class, rarity, name, base_type,
                    item_level, requires_level, quality, sockets, properties, requirements,
                    advanced FROM items WHERE id = ?1",
                params![id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, Option<String>>(5)?,
                        row.get::<_, Option<String>>(6)?,
                        row.get::<_, Option<String>>(7)?,
                        row.get::<_, Option<i64>>(8)?,
                        row.get::<_, Option<i64>>(9)?,
                        row.get::<_, Option<i64>>(10)?,
                        row.get::<_, Option<String>>(11)?,
                        row.get::<_, String>(12)?,
                        row.get::<_, String>(13)?,
                        row.get::<_, i64>(14)?,
                    ))
                },
            )
            .optional()?;

        let Some(row) = row else {
            return Ok(None);
        };

        Ok(Some(StoredItem {
            id: row.0,
            captured_ts: row.1,
            raw_text: row.2,
            source: row.3,
            item_class: row.4,
            rarity: row.5,
            name: row.6,
            base_type: row.7,
            item_level: row.8,
            requires_level: row.9,
            quality: row.10,
            sockets: row.11,
            properties: serde_json::from_str(&row.12).unwrap_or_default(),
            requirements: serde_json::from_str(&row.13).unwrap_or_default(),
            advanced: row.14 != 0,
            mods: self.mods_of(id)?,
        }))
    }

    fn mods_of(&self, item_id: i64) -> Result<Vec<StoredMod>> {
        let mut stmt = self.conn.prepare(
            "SELECT position, effect_index, kind, mod_name, tier, tags, text, value,
                value_min, value_max FROM item_mods WHERE item_id = ?1
             ORDER BY position, effect_index",
        )?;
        let rows = stmt
            .query_map(params![item_id], |row| {
                let tags: String = row.get(5)?;
                Ok(StoredMod {
                    position: row.get(0)?,
                    effect_index: row.get(1)?,
                    kind: row.get(2)?,
                    mod_name: row.get(3)?,
                    tier: row.get(4)?,
                    tags: serde_json::from_str(&tags).unwrap_or_default(),
                    text: row.get(6)?,
                    value: row.get(7)?,
                    value_min: row.get(8)?,
                    value_max: row.get(9)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<StoredMod>>>()?;
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::items::parse_item;
    use chrono::Utc;

    const SCEPTRE: &str = include_str!("fixtures/sceptre.txt");

    fn store() -> Poe2Store {
        Poe2Store::in_memory().unwrap()
    }

    #[test]
    fn item_is_saved_with_its_mods() {
        let mut s = store();
        let parsed = parse_item(SCEPTRE).unwrap();
        let (id, created) = s.add_item(&parsed, "paste", Utc::now()).unwrap();
        assert!(created);

        let saved = s.item(id).unwrap().unwrap();
        assert_eq!(saved.name.as_deref(), Some("Wrath Call"));
        assert_eq!(saved.base_type.as_deref(), Some("Rattling Sceptre"));
        assert_eq!(saved.item_level, Some(58));
        assert_eq!(saved.quality, Some(20));
        assert!(saved.advanced);
        assert_eq!(saved.source, "paste");
        assert_eq!(
            saved.properties.get("Spirit").map(String::as_str),
            Some("166")
        );
    }

    #[test]
    fn multiline_mod_keeps_one_position_for_two_effects() {
        let mut s = store();
        let parsed = parse_item(SCEPTRE).unwrap();
        let (id, _) = s.add_item(&parsed, "paste", Utc::now()).unwrap();
        let saved = s.item(id).unwrap().unwrap();
        let counselor: Vec<&StoredMod> = saved
            .mods
            .iter()
            .filter(|m| m.mod_name.as_deref() == Some("Counselor's"))
            .collect();
        assert_eq!(counselor.len(), 2);
        assert_eq!(counselor[0].position, counselor[1].position);
        assert_eq!(counselor[0].effect_index, 0);
        assert_eq!(counselor[1].effect_index, 1);
    }

    #[test]
    fn mod_values_and_ranges_are_stored() {
        let mut s = store();
        let parsed = parse_item(SCEPTRE).unwrap();
        let (id, _) = s.add_item(&parsed, "paste", Utc::now()).unwrap();
        let saved = s.item(id).unwrap().unwrap();
        let counts = saved
            .mods
            .iter()
            .find(|m| m.mod_name.as_deref() == Some("Count's"))
            .unwrap();
        assert_eq!(counts.tier, Some(4));
        assert_eq!(counts.value, Some(50.0));
        assert_eq!(counts.value_min, Some(45.0));
        assert_eq!(counts.value_max, Some(50.0));
        assert_eq!(counts.kind, "prefix");
    }

    #[test]
    fn tags_survive_the_roundtrip() {
        let mut s = store();
        let parsed = parse_item(SCEPTRE).unwrap();
        let (id, _) = s.add_item(&parsed, "paste", Utc::now()).unwrap();
        let saved = s.item(id).unwrap().unwrap();
        let overseer = saved
            .mods
            .iter()
            .find(|m| m.mod_name.as_deref() == Some("of the Overseer"))
            .unwrap();
        assert_eq!(overseer.tags, vec!["Minion".to_string(), "Gem".to_string()]);
    }

    #[test]
    fn same_item_twice_is_stored_once() {
        let mut s = store();
        let parsed = parse_item(SCEPTRE).unwrap();
        let (first_id, first_created) = s.add_item(&parsed, "paste", Utc::now()).unwrap();
        let (second_id, second_created) = s.add_item(&parsed, "clipboard", Utc::now()).unwrap();
        assert!(first_created);
        assert!(!second_created);
        assert_eq!(first_id, second_id);
        assert_eq!(s.items(50).unwrap().len(), 1);
    }

    #[test]
    fn raw_text_is_stored_verbatim() {
        let mut s = store();
        let parsed = parse_item(SCEPTRE).unwrap();
        let (id, _) = s.add_item(&parsed, "paste", Utc::now()).unwrap();
        assert_eq!(s.item(id).unwrap().unwrap().raw_text, SCEPTRE);
        assert_eq!(s.raw_items().unwrap(), vec![(id, SCEPTRE.to_string())]);
    }

    #[test]
    fn reparse_replaces_structure_but_not_text() {
        let mut s = store();
        let parsed = parse_item(SCEPTRE).unwrap();
        let (id, _) = s.add_item(&parsed, "paste", Utc::now()).unwrap();

        let changed_text = SCEPTRE.replace("Item Level: 58", "Item Level: 60");
        let changed = parse_item(&changed_text).unwrap();
        s.reparse_item(id, &changed).unwrap();

        let saved = s.item(id).unwrap().unwrap();
        assert_eq!(saved.item_level, Some(60));
        assert_eq!(saved.raw_text, SCEPTRE);
        // Modifiers did not double up: old rows are deleted before new ones land.
        let counselor_rows = saved
            .mods
            .iter()
            .filter(|m| m.mod_name.as_deref() == Some("Counselor's"))
            .count();
        assert_eq!(counselor_rows, 2);
    }

    #[test]
    fn items_are_listed_newest_first() {
        let mut s = store();
        let first = parse_item(SCEPTRE).unwrap();
        s.add_item(&first, "paste", Utc::now()).unwrap();
        let second = parse_item(&SCEPTRE.replace("Wrath Call", "Second Item")).unwrap();
        s.add_item(&second, "paste", Utc::now()).unwrap();

        let names: Vec<Option<String>> = s.items(50).unwrap().into_iter().map(|i| i.name).collect();
        assert_eq!(
            names,
            vec![
                Some("Second Item".to_string()),
                Some("Wrath Call".to_string())
            ]
        );
    }

    #[test]
    fn unknown_item_id_gives_none() {
        assert!(store().item(999).unwrap().is_none());
    }

    /// Deletes a file on drop even if the test panics, so a failed assertion
    /// doesn't leave scratch `.db` files behind in the OS temp directory.
    struct TempDbFile(std::path::PathBuf);

    impl Drop for TempDbFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    fn temp_db_path(label: &str) -> TempDbFile {
        let path = std::env::temp_dir().join(format!(
            "poe2_store_test_{label}_{}_{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_file(&path);
        TempDbFile(path)
    }

    /// A genuine failure — forced here by a second connection holding an
    /// exclusive lock on the same file, rather than a hook added to
    /// `store.rs` for the test's sake — must not leave anything behind.
    ///
    /// What this covers: SQLite's transaction is deferred, so it takes the
    /// write lock at its *first* write statement (the `UPDATE`) and holds it
    /// for every statement after. Contention from the blocker connection
    /// therefore fails the write before it starts, proving only that a write
    /// that never begins changes nothing.
    /// What this does NOT cover: a failure between the `DELETE` and the
    /// re-`INSERT` inside the same transaction — the exact window the
    /// transaction wrapping exists to make safe. There is no way to seize the
    /// lock mid-transaction from outside once the first statement has taken
    /// it, so that rollback behavior is covered by code inspection (the
    /// `DELETE` and `insert_mods` share one `Transaction` and only `commit()`
    /// makes either durable) rather than by this test.
    #[test]
    fn write_that_cannot_start_leaves_the_store_unchanged() {
        let db = temp_db_path("reparse");
        let mut s = Poe2Store::open(&db.0).unwrap();
        let parsed = parse_item(SCEPTRE).unwrap();
        let (id, _) = s.add_item(&parsed, "paste", Utc::now()).unwrap();

        // A second connection to the same file holds an exclusive lock, so the
        // store's transaction cannot acquire a write lock and the whole
        // reparse fails atomically instead of partially applying.
        let blocker = Connection::open(&db.0).unwrap();
        blocker.execute_batch("BEGIN EXCLUSIVE;").unwrap();

        let changed_text = SCEPTRE.replace("Item Level: 58", "Item Level: 60");
        let changed = parse_item(&changed_text).unwrap();
        let result = s.reparse_item(id, &changed);
        assert!(result.is_err(), "expected the locked write to fail");

        blocker.execute_batch("ROLLBACK;").unwrap();
        drop(blocker);

        let saved = s.item(id).unwrap().unwrap();
        // The old value and the full set of modifiers survived the failed write.
        assert_eq!(saved.item_level, Some(58));
        let counselor_rows = saved
            .mods
            .iter()
            .filter(|m| m.mod_name.as_deref() == Some("Counselor's"))
            .count();
        assert_eq!(counselor_rows, 2);
    }

    /// Same technique applied to `add_item`, and the same scope limit applies:
    /// this shows a write that cannot start creates nothing. It does not
    /// exercise a failure between the `items` insert and `insert_mods` inside
    /// the transaction, since the blocker connection can only contend for the
    /// lock before the first write statement takes it.
    #[test]
    fn add_item_that_cannot_start_creates_nothing() {
        let db = temp_db_path("add");
        let mut s = Poe2Store::open(&db.0).unwrap();

        let blocker = Connection::open(&db.0).unwrap();
        blocker.execute_batch("BEGIN EXCLUSIVE;").unwrap();

        let parsed = parse_item(SCEPTRE).unwrap();
        let result = s.add_item(&parsed, "paste", Utc::now());
        assert!(result.is_err(), "expected the locked write to fail");

        blocker.execute_batch("ROLLBACK;").unwrap();
        drop(blocker);

        assert_eq!(s.items(50).unwrap().len(), 0);
    }

    /// Regression test for moving the duplicate check inside `add_item`'s
    /// transaction: a duplicate must still return `(existing_id, false)` and
    /// write nothing — no second item row, no second set of modifier rows —
    /// rather than surfacing a raw `UNIQUE constraint failed` error.
    #[test]
    fn duplicate_inside_transaction_returns_existing_id_and_writes_nothing() {
        let mut s = store();
        let parsed = parse_item(SCEPTRE).unwrap();
        let (first_id, first_created) = s.add_item(&parsed, "paste", Utc::now()).unwrap();
        assert!(first_created);
        let mods_before = s.item(first_id).unwrap().unwrap().mods.len();

        let (second_id, second_created) = s.add_item(&parsed, "clipboard", Utc::now()).unwrap();
        assert_eq!(second_id, first_id);
        assert!(!second_created);

        assert_eq!(s.items(50).unwrap().len(), 1);
        let mods_after = s.item(first_id).unwrap().unwrap().mods.len();
        assert_eq!(mods_before, mods_after);
    }

    /// The happy path this fix must not regress: a successful reparse still
    /// leaves the full modifier set in place.
    #[test]
    fn successful_reparse_leaves_full_modifier_count() {
        let mut s = store();
        let parsed = parse_item(SCEPTRE).unwrap();
        let (id, _) = s.add_item(&parsed, "paste", Utc::now()).unwrap();
        let before = s.item(id).unwrap().unwrap().mods.len();

        s.reparse_item(id, &parsed).unwrap();

        let after = s.item(id).unwrap().unwrap().mods.len();
        assert_eq!(before, after);
        assert!(after > 0);
    }
}
