//! Polling the system clipboard for items.
//!
//! Polling, not interception. Global keyboard hooks look like a keylogger to
//! antivirus software and invite questions about the game's rules; reading the
//! clipboard once a second is enough.
//!
//! This module only reads. Nothing is ever written to the clipboard.
//!
//! Free of Tauri types: the reader and the item callback are injected, so
//! this can be tested without a running app or a real clipboard. The Tauri
//! wiring (`spawn`) lives in the `handy` crate, which owns the `AppHandle`.

use std::time::Duration;

use crate::items::looks_like_item;

pub const POLL_INTERVAL: Duration = Duration::from_secs(1);

pub struct ClipboardWatcher<R, F>
where
    R: FnMut() -> Result<Option<String>, String>,
    F: FnMut(String),
{
    read: R,
    on_item: F,
    last: Option<String>,
    available: bool,
}

impl<R, F> ClipboardWatcher<R, F>
where
    R: FnMut() -> Result<Option<String>, String>,
    F: FnMut(String),
{
    pub fn new(read: R, on_item: F) -> Self {
        ClipboardWatcher {
            read,
            on_item,
            last: None,
            available: true,
        }
    }

    pub fn available(&self) -> bool {
        self.available
    }

    /// One check. True means an item was found and handed over.
    pub fn check_once(&mut self) -> bool {
        let text = match (self.read)() {
            Ok(Some(text)) => text,
            Ok(None) => return false,
            Err(_) => {
                // Clipboard unavailable: watching switches off, manual pasting
                // still works.
                self.available = false;
                return false;
            }
        };

        if text.is_empty() || self.last.as_deref() == Some(text.as_str()) {
            return false;
        }
        self.last = Some(text.clone());

        // A strict test: a password copied out of a password manager must never
        // even reach the parser.
        if !looks_like_item(&text) {
            return false;
        }
        (self.on_item)(text);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    const SCEPTRE: &str = "Item Class: Sceptres\nRarity: Rare\nWrath Call\nRattling Sceptre\n";

    /// A watcher over a fake clipboard, handing out texts one per check.
    fn watcher_over(
        texts: Vec<Option<String>>,
        seen: Rc<RefCell<Vec<String>>>,
    ) -> ClipboardWatcher<impl FnMut() -> Result<Option<String>, String>, impl FnMut(String)> {
        let mut queue = texts.into_iter();
        ClipboardWatcher::new(
            move || Ok(queue.next().flatten()),
            move |text| seen.borrow_mut().push(text),
        )
    }

    #[test]
    fn item_text_is_picked_up() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let mut watcher = watcher_over(vec![Some(SCEPTRE.to_string())], seen.clone());
        assert!(watcher.check_once());
        assert_eq!(seen.borrow().as_slice(), &[SCEPTRE.to_string()]);
    }

    #[test]
    fn anything_else_is_ignored_silently() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let mut watcher = watcher_over(
            vec![Some("my password from the password manager".to_string())],
            seen.clone(),
        );
        assert!(!watcher.check_once());
        assert!(seen.borrow().is_empty());
    }

    #[test]
    fn empty_clipboard_is_ignored() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let mut watcher = watcher_over(vec![None], seen.clone());
        assert!(!watcher.check_once());
        assert!(seen.borrow().is_empty());
    }

    #[test]
    fn the_same_text_is_not_taken_twice() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let mut watcher = watcher_over(
            vec![Some(SCEPTRE.to_string()), Some(SCEPTRE.to_string())],
            seen.clone(),
        );
        watcher.check_once();
        assert!(!watcher.check_once());
        assert_eq!(seen.borrow().len(), 1);
    }

    #[test]
    fn a_new_item_after_a_repeat_is_taken() {
        let other = SCEPTRE.replace("Wrath Call", "Second Item");
        let seen = Rc::new(RefCell::new(Vec::new()));
        let mut watcher = watcher_over(
            vec![
                Some(SCEPTRE.to_string()),
                Some(SCEPTRE.to_string()),
                Some(other),
            ],
            seen.clone(),
        );
        watcher.check_once();
        watcher.check_once();
        watcher.check_once();
        assert_eq!(seen.borrow().len(), 2);
    }

    #[test]
    fn unreadable_clipboard_switches_watching_off() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let mut watcher = ClipboardWatcher::new(
            || Err("clipboard busy in another application".to_string()),
            move |text| seen.borrow_mut().push(text),
        );
        assert!(!watcher.check_once());
        assert!(!watcher.available());
    }

    /// `Ok(None)` is the contract for "no text right now" — a copied
    /// screenshot or file, not a real failure — and must never trip
    /// `available()` off. The watcher keeps polling and still picks up an
    /// item that shows up afterwards.
    ///
    /// What this test does NOT cover: `check_once`'s handling of `Ok(None)`
    /// was already correct before the screenshot bug was fixed, and is
    /// already pinned by `empty_clipboard_is_ignored` above. The actual bug
    /// lived in `watcher.rs`'s reader closure — the code that turns a real
    /// `tauri_plugin_clipboard_manager::read_text()` failure into `Ok(None)`
    /// instead of `Err` — and that closure needs a real `AppHandle`, so no
    /// unit test in this crate can reach it. That mapping is guaranteed by
    /// inspection of the closure and its doc comment, not by a test, the same
    /// way some invariants in `store.rs` are documented rather than asserted.
    #[test]
    fn no_text_is_not_an_error_and_watching_keeps_going() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let mut watcher = watcher_over(vec![None, None, Some(SCEPTRE.to_string())], seen.clone());
        assert!(!watcher.check_once());
        assert!(watcher.available());
        assert!(!watcher.check_once());
        assert!(watcher.available());
        assert!(watcher.check_once());
        assert!(watcher.available());
        assert_eq!(seen.borrow().as_slice(), &[SCEPTRE.to_string()]);
    }
}
