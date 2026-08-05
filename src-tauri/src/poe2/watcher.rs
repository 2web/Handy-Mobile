//! Tauri wiring for the clipboard watcher.
//!
//! The polling logic lives in `poe2_core::clipboard_watch`, free of any Tauri
//! type. This module only supplies the `AppHandle`-dependent bits: reading the
//! real system clipboard, storing parsed items, and re-checking the setting.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, Emitter};
use tauri_plugin_clipboard_manager::ClipboardExt;

use poe2_core::clipboard_watch::{ClipboardWatcher, POLL_INTERVAL};
use poe2_core::items::parse_item;

use crate::settings;

/// Guards against two polling threads running at once. `spawn` is called both
/// at startup and whenever the setting is switched on mid-session, so a second
/// call while a thread is already running must be a no-op rather than starting
/// a duplicate. Once set, this stays `true` for the life of the process on the
/// normal path: the thread it guards never exits just because the setting
/// went off (see the loop below), so there is no window for a racing `spawn`
/// call to find the flag cleared and start a second thread.
static WATCHER_RUNNING: AtomicBool = AtomicBool::new(false);

/// Emitted after a clipboard capture is successfully written to the store.
/// Carries no payload: `ItemsPage` just refetches its list on receipt, the
/// same way the page already refreshes after a manual paste.
pub const ITEM_CAPTURED_EVENT: &str = "poe2://item-captured";

/// Starts the polling thread if the setting is on and no thread is already
/// running. Called once at startup, and again from the settings command
/// whenever the toggle is switched on, so enabling clipboard watching takes
/// effect immediately instead of requiring a restart. The thread it starts
/// lives for the rest of the process (barring a genuine clipboard failure):
/// later disabling the setting does not stop it, only pauses its reads.
pub fn spawn(app: AppHandle) {
    let settings = settings::get_settings(&app);
    if !(settings.poe2_enabled && settings.poe2_clipboard_watch) {
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
                if store
                    .add_item(&parsed, "clipboard", chrono::Utc::now())
                    .is_ok()
                {
                    // Only on a successful store: the page must refetch when a
                    // capture actually landed, not on a write failure it can't
                    // do anything about anyway.
                    let _ = store_app.emit(ITEM_CAPTURED_EVENT, ());
                }
            },
        );

        while watcher.available() {
            // Skip the read rather than exiting when the setting is off: a
            // `spawn` call from `change_poe2_clipboard_watch_setting` racing
            // this thread's exit could see `WATCHER_RUNNING` still `true` and
            // return without starting a replacement, leaving the setting on
            // and nothing watching until the user happens to toggle it again.
            // One thread lives for the process once started; a sleeping
            // thread costs nothing, and while the setting is off the
            // clipboard is still never read, so the privacy commitment holds.
            let settings = settings::get_settings(&app);
            if settings.poe2_enabled && settings.poe2_clipboard_watch {
                watcher.check_once();
            }
            std::thread::sleep(POLL_INTERVAL);
        }

        // Only reached via `available() == false`, i.e. a genuine clipboard
        // read failure reported by the injected reader. In production every
        // plugin read failure is mapped to `Ok(None)` (see the reader closure
        // above), so this path is unreachable there and exercised only by
        // tests with an injected reader; clearing the flag here — and nowhere
        // else — is deliberate, not an oversight.
        WATCHER_RUNNING.store(false, Ordering::SeqCst);
    });
}
