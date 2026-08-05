//! Tauri commands for the Path of Exile 2 section.

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;

use poe2_core::items::parse_item;
use poe2_core::store::{Poe2Store, StoredItem};

use crate::settings;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct AddItemResult {
    pub id: i64,
    pub created: bool,
    pub item: Option<StoredItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct RebuildResult {
    pub reparsed: u32,
    pub failed: u32,
}

/// Opens the item database. A fresh connection per call: opening a SQLite file
/// is cheap, and a per-call connection keeps the commands free of shared state.
pub fn store_for(app: &AppHandle) -> Result<Poe2Store, String> {
    let dir = crate::portable::app_data_dir(app).map_err(|e| e.to_string())?;
    Poe2Store::open(&dir.join("poe2.db")).map_err(|e| e.to_string())
}

/// The fixed error text `poe2_add_item` returns when the pasted text does not
/// parse as an item. Exported so the frontend can match on it and show a
/// different message than a genuine storage failure — matching a known,
/// fixed string here is simpler and less brittle than parsing prose out of
/// an arbitrary storage error on the frontend.
pub const NOT_AN_ITEM_ERROR: &str =
    "That does not look like an item. Hover it in the game and press Ctrl+C.";

#[tauri::command]
#[specta::specta]
pub fn poe2_add_item(app: AppHandle, text: String) -> Result<AddItemResult, String> {
    let parsed = parse_item(&text).map_err(|_| NOT_AN_ITEM_ERROR.to_string())?;

    let mut store = store_for(&app)?;
    let (id, created) = store
        .add_item(&parsed, "paste", chrono::Utc::now())
        .map_err(|e| e.to_string())?;
    let item = store.item(id).map_err(|e| e.to_string())?;
    Ok(AddItemResult { id, created, item })
}

#[tauri::command]
#[specta::specta]
pub fn poe2_list_items(app: AppHandle) -> Result<Vec<StoredItem>, String> {
    store_for(&app)?.items(50).map_err(|e| e.to_string())
}

/// Reparses stored items from their raw text.
///
/// Raw text is the source of truth and structure is derived. When the parser
/// gets smarter — and it will — stored items reparse themselves with no
/// re-pasting.
#[tauri::command]
#[specta::specta]
pub fn poe2_rebuild_items(app: AppHandle) -> Result<RebuildResult, String> {
    let mut store = store_for(&app)?;
    let mut reparsed = 0u32;
    let mut failed = 0u32;
    for (id, raw_text) in store.raw_items().map_err(|e| e.to_string())? {
        // A record that no longer parses must not abort the rebuild of all the
        // others, but it also must not vanish silently: a raw text that stops
        // parsing means either the parser regressed or the row is corrupt, and
        // both are worth knowing about. Never log the raw text itself — it is
        // player data and the log is a file on disk.
        match parse_item(&raw_text) {
            Ok(parsed) => {
                store.reparse_item(id, &parsed).map_err(|e| e.to_string())?;
                reparsed += 1;
            }
            Err(_) => {
                log::warn!("poe2_rebuild_items: item {id} no longer parses, skipping");
                failed += 1;
            }
        }
    }
    Ok(RebuildResult { reparsed, failed })
}

#[tauri::command]
#[specta::specta]
pub fn change_poe2_enabled_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.poe2_enabled = enabled;
    settings::write_settings(&app, settings.clone());
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_poe2_clipboard_watch_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.poe2_clipboard_watch = enabled;
    settings::write_settings(&app, settings.clone());
    if enabled {
        // Starts the polling thread immediately rather than making the user
        // restart the app to see the toggle they just turned on take effect.
        // `spawn` itself guards against a duplicate thread if one is already
        // running.
        crate::poe2::watcher::spawn(app);
    }
    Ok(())
}
