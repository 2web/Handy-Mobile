//! Tauri wiring for the clipboard watcher.
//!
//! The polling logic lives in `poe2_core::clipboard_watch`, free of any Tauri
//! type. This module only supplies the `AppHandle`-dependent bits: reading the
//! real system clipboard, storing parsed items, and re-checking the setting.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::AppHandle;
use tauri_plugin_clipboard_manager::ClipboardExt;

use poe2_core::clipboard_watch::{ClipboardWatcher, POLL_INTERVAL};
use poe2_core::items::parse_item;

use crate::settings;

/// Guards against two polling threads running at once. `spawn` is called both
/// at startup and whenever the setting is switched on mid-session, so a second
/// call while a thread is already running must be a no-op rather than starting
/// a duplicate.
static WATCHER_RUNNING: AtomicBool = AtomicBool::new(false);

/// Starts the polling thread if the setting is on and no thread is already
/// running. Called once at startup, and again from the settings command
/// whenever the toggle is switched on, so enabling clipboard watching takes
/// effect immediately instead of requiring a restart.
pub fn spawn(app: AppHandle) {
    if !settings::get_settings(&app).poe2_clipboard_watch {
        return;
    }

    if WATCHER_RUNNING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        // Already running: enabling, disabling and re-enabling within one
        // session must never leave two threads polling the clipboard.
        return;
    }

    std::thread::spawn(move || {
        let reader_app = app.clone();
        let store_app = app.clone();
        let mut watcher = ClipboardWatcher::new(
            move || {
                // A read failure here almost always means the clipboard holds
                // non-text content (a screenshot, a copied file) rather than a
                // real access error: `tauri_plugin_clipboard_manager` collapses
                // every underlying `arboard::Error` variant into one opaque
                // string, so there is no reliable way to tell "not text" apart
                // from "genuinely unavailable". Treating every failure as "no
                // text right now" means a copied screenshot is silently skipped
                // instead of permanently killing the watcher; a real, lasting
                // clipboard failure just shows up as "no text" on every poll
                // forever, which is harmless since nothing is ever taken from
                // it. `available()`/`Err` still exist in `ClipboardWatcher` for
                // injected readers that need a genuine fatal path.
                Ok(reader_app.clipboard().read_text().ok())
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

        WATCHER_RUNNING.store(false, Ordering::SeqCst);
    });
}
