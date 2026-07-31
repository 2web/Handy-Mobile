# Tray Translation-Language Switcher — Design

**Date:** 2026-07-31
**Status:** Approved (design), pending spec review

## Goal

Let the user switch the translation target language quickly from the system tray, instead of opening Settings → Translation and scrolling the ~40-item dropdown. The tray shows a curated shortlist of common languages with a checkmark on the active one; one click switches it.

## Scope & decisions (locked with user)

- **Tray only.** No hotkey, no popup window, no new persistent setting, no favorites-management UI.
- **Curated shortlist** of ~14 common languages (not the full ~40 list). Exotic languages remain reachable via the existing Settings dropdown.
- Reuses the existing **shared** `translation_target_language` setting — so switching affects BOTH voice translation and clipboard translation. (Separate languages per feature are explicitly out of scope.)
- Language item labels are **English names** (e.g. "German"), reusing the same code→name mapping the translation prompt uses, for consistency and zero extra i18n.
- The submenu **title** is localized (en + ru at minimum; other locales fall back).

## Architecture

Everything is backend. Three edit sites plus i18n:

1. **`src-tauri/src/tray.rs`** — a curated shortlist constant, a pure helper that computes the menu entries, and the submenu construction inside `update_tray_menu`.
2. **`src-tauri/src/lib.rs`** — a `translate_lang:{code}` arm in the tray `on_menu_event` handler.
3. **`src-tauri/src/actions.rs`** — make `language_english_name` `pub(crate)` so the tray reuses it (currently private).
4. **i18n** — add a `translationLanguage` key to the `tray` section of `src/i18n/locales/en/translation.json` (defines the generated `TrayStrings` field) and `ru/translation.json`.

### Component 1: shortlist + pure helper (tray.rs)

```rust
/// Curated common translation targets shown in the tray language submenu.
/// Codes must be ones `language_english_name` maps (so labels and the
/// translation prompt agree). Exotic languages stay reachable via Settings.
const TRANSLATION_LANGUAGE_SHORTLIST: &[&str] = &[
    "en", "de", "ru", "es", "fr", "it", "pt", "ja", "ko", "uk", "pl", "tr", "nl", "zh-Hans",
];

/// Ordered menu entries for the language submenu: each shortlist code plus,
/// if the current target is not in the shortlist, the current code appended so
/// the active language is always visible and checked. Returns (code, is_checked).
pub(crate) fn language_menu_entries(shortlist: &[&str], current: &str) -> Vec<(String, bool)> {
    let mut entries: Vec<(String, bool)> =
        shortlist.iter().map(|c| (c.to_string(), *c == current)).collect();
    if !shortlist.iter().any(|c| *c == current) {
        entries.push((current.to_string(), true));
    }
    entries
}
```

Labels are resolved at render time via `crate::actions::language_english_name(&code)` (falls back to the raw code for unknown values).

### Component 2: submenu in `update_tray_menu` (tray.rs)

Built like the existing `model_submenu`, added to the **Idle** menu only (same as the model submenu — the Recording/Transcribing menus stay minimal). The submenu **title** is the localized static label `strings.translation_language` ("Translation Language" / "Язык перевода"); the active language is conveyed by the checkmark on its item (not by the submenu title). This differs deliberately from the model submenu — which shows the active model name as its title only because it has no localized wrapper label; here we have one, so we use it.

```rust
let current_lang = settings.translation_target_language.clone();
let language_submenu = {
    let submenu = Submenu::with_id(app, "translation_language_submenu", &strings.translation_language, true)
        .expect("failed to create translation language submenu");
    for (code, is_active) in language_menu_entries(TRANSLATION_LANGUAGE_SHORTLIST, &current_lang) {
        let label = crate::actions::language_english_name(&code);
        let item_id = format!("translate_lang:{}", code);
        let item = CheckMenuItem::with_id(app, &item_id, &label, true, is_active, None::<&str>)
            .expect("failed to create language item");
        let _ = submenu.append(&item);
    }
    submenu
};
```

Placed in the Idle `Menu::with_items` list next to `model_submenu` / `unload_model_i` (same separator group), before the Settings group.

### Component 3: menu event handler (lib.rs)

Add an arm alongside `model_select:`:

```rust
id if id.starts_with("translate_lang:") => {
    let code = id.strip_prefix("translate_lang:").unwrap().to_string();
    let mut settings = settings::get_settings(app);
    if settings.translation_target_language == code {
        return;
    }
    settings.translation_target_language = code.clone();
    settings::write_settings(app, settings);
    let _ = app.emit(
        "settings-changed",
        serde_json::json!({ "setting": "translation_target_language", "value": code }),
    );
    log::info!("Translation language switched to {} via tray.", code);
    tray::update_tray_menu(app, None);
}
```

`app.emit` requires `tauri::Emitter` in scope (already used elsewhere in lib.rs — verify import).

### Component 4: i18n

Add to the `tray` object in `src/i18n/locales/en/translation.json`:

```json
"translationLanguage": "Translation Language"
```

and in `src/i18n/locales/ru/translation.json`:

```json
"translationLanguage": "Язык перевода"
```

The build script (`src-tauri/build.rs::generate_tray_translations`) turns the English `tray` keys into `TrayStrings` fields; `translationLanguage` → `TrayStrings.translation_language`. Locales missing the key fall back per the existing generator behavior.

### Component 5: expose `language_english_name` (actions.rs)

Change `fn language_english_name` to `pub(crate) fn language_english_name`. No behavior change. Its existing unit tests stay valid.

## Data flow

1. `update_tray_menu` reads `translation_target_language`, builds the submenu with a checkmark on the active code.
2. User clicks a language item → `on_menu_event` writes the setting, emits `settings-changed`, rebuilds the tray menu (checkmark moves).
3. The next voice or clipboard translation reads `translation_target_language` and uses the new value. No restart, no reload.

## Error handling

- Unknown/edge code: `language_english_name` falls back to the raw code; the setting still stores it. No panic.
- `settings::write_settings` is the same persistence path used everywhere; failures are handled as elsewhere (it does not return a Result to callers here).
- Re-clicking the active language early-returns (no rebuild).

## Testing

Unit tests in `tray.rs` for the pure helper (no `AppHandle` needed):

- `language_menu_entries` with `current` **in** the shortlist: returns exactly `shortlist.len()` entries, the current one is the only `is_checked == true`, no duplicate of the current code.
- `language_menu_entries` with `current` **not** in the shortlist (e.g. `"he"`): returns `shortlist.len() + 1` entries, the appended last entry is `("he", true)`, and no shortlist entry is checked.

Menu construction and the event handler are thin glue over `AppHandle` and are verified by build + manual tray click (switch language, confirm checkmark moves and a subsequent translation uses it), consistent with how the existing `model_select` tray flow is validated. Note: automated `cargo test` execution is blocked in this environment (pre-existing `0xc0000139` test-binary launch issue); correctness rests on the helper unit tests (by inspection/compile) + manual tray verification.

## Out of scope (YAGNI)

- Hotkey cycling, popup picker, per-feature (voice vs clipboard) separate languages, favorites management UI, localizing individual language names, showing the full ~40-language list in the tray.

## Files touched

- `src-tauri/src/tray.rs` (shortlist, helper, submenu, tests)
- `src-tauri/src/lib.rs` (event handler arm)
- `src-tauri/src/actions.rs` (visibility of `language_english_name`)
- `src/i18n/locales/en/translation.json`, `src/i18n/locales/ru/translation.json` (`tray.translationLanguage`)
