//! Tauri wiring for the clipboard watcher.
//!
//! The polling logic lives in `poe2_core::clipboard_watch`, free of any Tauri
//! type. This module only supplies the `AppHandle`-dependent bits: reading the
//! real system clipboard, storing parsed items, and re-checking the setting.

use tauri::AppHandle;
use tauri_plugin_clipboard_manager::ClipboardExt;

use poe2_core::clipboard_watch::{ClipboardWatcher, POLL_INTERVAL};
use poe2_core::items::parse_item;

use crate::settings;

/// Starts the polling thread if the setting is on. Called once at startup.
pub fn spawn(app: AppHandle) {
    if !settings::get_settings(&app).poe2_clipboard_watch {
        return;
    }

    std::thread::spawn(move || {
        let reader_app = app.clone();
        let store_app = app.clone();
        let mut watcher = ClipboardWatcher::new(
            move || {
                reader_app
                    .clipboard()
                    .read_text()
                    .map(Some)
                    .map_err(|e| e.to_string())
            },
            move |text| {
                let Ok(parsed) = parse_item(&text) else {
                    return;
                };
                let Ok(mut store) = crate::poe2::commands::store_for(&store_app) else {
                    return;
                };
                let _ = store.add_item(&parsed, "clipboard", chrono::Utc::now());
            },
        );

        while watcher.available() {
            if !settings::get_settings(&app).poe2_clipboard_watch {
                break;
            }
            watcher.check_once();
            std::thread::sleep(POLL_INTERVAL);
        }
    });
}
