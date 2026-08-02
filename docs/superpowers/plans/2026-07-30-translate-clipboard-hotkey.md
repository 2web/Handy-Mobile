# Translate-Clipboard Hotkey Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a global hotkey that reads the text currently in the clipboard, translates it via the existing local-LLM pipeline, and pastes the result at the cursor.

**Architecture:** A new one-shot `ShortcutAction` (`ClipboardTranslateAction`) registered in `ACTION_MAP` under the id `translate_clipboard`. It reuses the existing `run_llm` + `build_translation_prompt` translation path (no audio, no recording coordinator) and the existing `utils::paste` output path. A new default `ShortcutBinding` makes it appear and register like every other shortcut; the frontend gets a `ShortcutInput` in the Translation settings group plus en/ru labels.

**Tech Stack:** Rust (Tauri 2 backend), React/TypeScript (frontend), i18next, `tauri-plugin-clipboard-manager`.

## Global Constraints

- Push code ONLY to `origin` (2web/Handy-Mobile). NEVER to `upstream`.
- Windows build uses a short target dir: `export CARGO_TARGET_DIR="C:/h"` before any `cargo`/`tauri` command (260-char path limit).
- Reuse the existing translation pipeline — do NOT duplicate `run_llm`, `build_translation_prompt`, or paste logic.
- The action must NOT go through `TranscriptionCoordinator` / recording — it is a stateless one-shot on key press.
- Default translation target language is the shared `settings.translation_target_language` (no new setting).
- Rust: `bun run --cwd src-tauri` is not used; run `cargo` from `src-tauri` (or `cargo test --manifest-path src-tauri/Cargo.toml`).

---

## File Structure

- `src-tauri/src/actions.rs` — add `ClipboardTranslateAction` struct + `impl ShortcutAction`, register it in `ACTION_MAP`, add a unit test. Add `use tauri_plugin_clipboard_manager::ClipboardExt;`.
- `src-tauri/src/settings.rs` — add the `translate_clipboard` default `ShortcutBinding` in `get_default_settings()`; extend the existing binding test.
- `src/components/settings/post-processing/PostProcessingSettings.tsx` — add a `ShortcutInput shortcutId="translate_clipboard"` to the Translation `SettingsGroup`.
- `src/i18n/locales/en/translation.json` and `src/i18n/locales/ru/translation.json` — add `settings.general.shortcut.bindings.translate_clipboard.{name,description}`.

No new files. `translate_clipboard` is deliberately NOT added to `is_transcribe_binding` (transcription_coordinator.rs) so the shared handler routes it through the simple start/stop path.

---

### Task 1: Backend action, registration, and default binding

**Files:**
- Modify: `src-tauri/src/actions.rs` (add import near line 25; add struct + impl after `CancelAction` ~line 935; register in `ACTION_MAP` ~line 975; add test in `mod tests`)
- Modify: `src-tauri/src/settings.rs` (add binding after the `transcribe_with_translation` insert, ~line 853; extend test ~line 1516)

**Interfaces:**
- Consumes (already defined in actions.rs): `trait ShortcutAction { fn start(&self, app: &AppHandle, binding_id: &str, shortcut_str: &str); fn stop(...); }`; `fn build_translation_prompt(target_language_code: &str) -> String`; `async fn run_llm(settings: &AppSettings, transcription: &str, system_prompt: String) -> Option<String>`; `crate::settings::get_settings(app) -> AppSettings`; `crate::utils::paste(text: String, app: AppHandle) -> Result<(), String>`.
- Produces: `ACTION_MAP["translate_clipboard"] -> Arc<dyn ShortcutAction>`; default `ShortcutBinding` with `id = "translate_clipboard"`.

- [ ] **Step 1: Write the failing test (actions.rs)**

Add to `mod tests` in `src-tauri/src/actions.rs`:

```rust
#[test]
fn action_map_contains_translate_clipboard() {
    assert!(super::ACTION_MAP.contains_key("translate_clipboard"));
}
```

- [ ] **Step 2: Write the failing test (settings.rs)**

Extend the existing test `default_settings_include_translation_target_and_binding` in `src-tauri/src/settings.rs` by adding a line inside it:

```rust
assert!(s.bindings.contains_key("translate_clipboard"));
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml action_map_contains_translate_clipboard default_settings_include_translation_target_and_binding`
Expected: FAIL — `translate_clipboard` key absent from `ACTION_MAP` and from default bindings.

- [ ] **Step 4: Add the clipboard import (actions.rs)**

Below the existing `use tauri::{AppHandle, Emitter};` (line 25) add:

```rust
use tauri_plugin_clipboard_manager::ClipboardExt;
```

- [ ] **Step 5: Add the action struct and impl (actions.rs)**

Insert after the `CancelAction` impl block (just before `// Test Action`):

```rust
// Clipboard Translate Action
//
// One-shot action: read the current clipboard text, translate it through the
// same local-LLM pipeline used by TranscribeMode::Translate, and paste the
// result at the cursor. No audio, no recording — it runs entirely on key press
// via the simple start/stop path in the shortcut handler.
struct ClipboardTranslateAction;

impl ShortcutAction for ClipboardTranslateAction {
    fn start(&self, app: &AppHandle, binding_id: &str, _shortcut_str: &str) {
        debug!("ClipboardTranslateAction::start called for binding: {}", binding_id);

        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let text = match app.clipboard().read_text() {
                Ok(text) => text,
                Err(e) => {
                    debug!("Clipboard translate skipped: failed to read clipboard text: {}", e);
                    return;
                }
            };

            if text.trim().is_empty() {
                debug!("Clipboard translate skipped: clipboard has no text");
                return;
            }

            let settings = get_settings(&app);
            let system_prompt = build_translation_prompt(&settings.translation_target_language);

            let Some(translated) = run_llm(&settings, &text, system_prompt).await else {
                debug!("Clipboard translate produced no output");
                return;
            };

            if translated.trim().is_empty() {
                debug!("Clipboard translate produced empty output; nothing to paste");
                return;
            }

            let app_for_paste = app.clone();
            app.run_on_main_thread(move || match utils::paste(translated, app_for_paste.clone()) {
                Ok(()) => debug!("Clipboard translation pasted successfully"),
                Err(e) => {
                    error!("Failed to paste clipboard translation: {}", e);
                    let _ = app_for_paste.emit("paste-error", ());
                }
            })
            .unwrap_or_else(|e| {
                error!("Failed to run clipboard-translate paste on main thread: {:?}", e);
            });
        });
    }

    fn stop(&self, _app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        // One-shot on press; nothing to do on release.
    }
}
```

- [ ] **Step 6: Register the action in ACTION_MAP (actions.rs)**

In the `ACTION_MAP` initializer, after the `transcribe_with_translation` insert and before the `cancel` insert, add:

```rust
map.insert(
    "translate_clipboard".to_string(),
    Arc::new(ClipboardTranslateAction) as Arc<dyn ShortcutAction>,
);
```

- [ ] **Step 7: Add the default binding (settings.rs)**

After the `transcribe_with_translation` `bindings.insert(...)` block (ends ~line 853) and before the `cancel` insert, add:

```rust
#[cfg(target_os = "windows")]
let default_clipboard_translation_shortcut = "ctrl+alt+shift+space";
#[cfg(target_os = "macos")]
let default_clipboard_translation_shortcut = "option+ctrl+shift+space";
#[cfg(target_os = "linux")]
let default_clipboard_translation_shortcut = "ctrl+alt+shift+space";
#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
let default_clipboard_translation_shortcut = "alt+ctrl+shift+space";

bindings.insert(
    "translate_clipboard".to_string(),
    ShortcutBinding {
        id: "translate_clipboard".to_string(),
        name: "Translate Clipboard".to_string(),
        description: "Translates the text currently in the clipboard and pastes the result."
            .to_string(),
        default_binding: default_clipboard_translation_shortcut.to_string(),
        current_binding: default_clipboard_translation_shortcut.to_string(),
    },
);
```

- [ ] **Step 8: Run the tests to verify they pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml action_map_contains_translate_clipboard default_settings_include_translation_target_and_binding`
Expected: PASS.

- [ ] **Step 9: Run the full backend test suite + lint**

Run: `cargo test --manifest-path src-tauri/Cargo.toml` then `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets`
Expected: all tests pass; no new clippy warnings from the added code.

- [ ] **Step 10: Commit**

```bash
git add src-tauri/src/actions.rs src-tauri/src/settings.rs
git commit -m "feat(shortcut): add translate-clipboard hotkey action and default binding"
```

---

### Task 2: Frontend shortcut UI and i18n labels

**Files:**
- Modify: `src/components/settings/post-processing/PostProcessingSettings.tsx` (Translation `SettingsGroup`, ~line 449)
- Modify: `src/i18n/locales/en/translation.json` (`settings.general.shortcut.bindings`, ~line 178)
- Modify: `src/i18n/locales/ru/translation.json` (same path)

**Interfaces:**
- Consumes: `ShortcutInput` component (already imported in `PostProcessingSettings.tsx`), the backend binding id `translate_clipboard` from Task 1, and the i18n lookup pattern `settings.general.shortcut.bindings.${shortcutId}.{name,description}` (GlobalShortcutInput.tsx:256–263).
- Produces: a visible, rebindable shortcut row for `translate_clipboard` in the Translation section.

- [ ] **Step 1: Add the ShortcutInput to the Translation group**

In `src/components/settings/post-processing/PostProcessingSettings.tsx`, inside the `<SettingsGroup title={t("settings.translation.title")}>` block, add a second `ShortcutInput` after the existing `transcribe_with_translation` one and before `<TranslationTargetLanguage .../>`:

```tsx
<ShortcutInput
  shortcutId="translate_clipboard"
  descriptionMode="tooltip"
  grouped={true}
/>
```

- [ ] **Step 2: Add the English i18n keys**

In `src/i18n/locales/en/translation.json`, inside `settings.general.shortcut.bindings` (after the `transcribe_with_post_process` object, adding a comma after its closing brace), add:

```json
"translate_clipboard": {
  "name": "Translate Clipboard Hotkey",
  "description": "Translates the text currently in the clipboard and pastes the result at the cursor."
}
```

- [ ] **Step 3: Add the Russian i18n keys**

In `src/i18n/locales/ru/translation.json`, at the same key path `settings.general.shortcut.bindings`, add:

```json
"translate_clipboard": {
  "name": "Перевод буфера обмена",
  "description": "Переводит текст, который сейчас находится в буфере обмена, и вставляет результат в позицию курсора."
}
```

If the `transcribe_with_post_process` binding object is absent in the ru file, add `translate_clipboard` as a new member of whatever `bindings` object exists there (create the `bindings` object only if it is missing), keeping valid JSON.

- [ ] **Step 4: Type-check and lint the frontend**

Run: `bunx tsc --noEmit` then `bun run lint`
Expected: no type errors; both JSON files parse (a JSON syntax error surfaces here or in the app).

- [ ] **Step 5: Commit**

```bash
git add src/components/settings/post-processing/PostProcessingSettings.tsx src/i18n/locales/en/translation.json src/i18n/locales/ru/translation.json
git commit -m "feat(ui): expose translate-clipboard hotkey in Translation settings with en/ru labels"
```

---

### Task 3: Build and manual end-to-end verification

**Files:** none (verification only)

**Interfaces:** Consumes the running app produced by the release build; relies on the running Ollama server (`qwen2.5:3b`, provider `custom`) already configured in this environment.

- [ ] **Step 1: Build the release app**

Run (from repo root, PowerShell): `$env:CARGO_TARGET_DIR="C:/h"; bun run tauri build`
Expected: build succeeds; `C:/h/release/handy.exe` is produced.

- [ ] **Step 2: Confirm Ollama generation endpoint is up**

Run: `curl -s -o NUL -w "%{http_code}" -X POST http://localhost:11434/v1/chat/completions -H "Content-Type: application/json" --data @"C:/Users/Andre/AppData/Local/Temp/claude/C--Users-Andre-Documents-Projects-handy/9ff82227-3cfe-4308-979c-f78aec99f870/scratchpad/ollama_test.json"`
Expected: `200`. If `500` ("llama-server binary not found"), reinstall Ollama per the `handy-ollama-translation-setup` memory before continuing.

- [ ] **Step 3: Manual E2E test**

1. Launch `C:/h/release/handy.exe`.
2. Copy a Russian sentence to the clipboard (e.g. select "Привет, как у тебя дела сегодня?" and Ctrl+C).
3. Click into any editable text field.
4. Press the hotkey `Ctrl+Alt+Shift+Space`.
5. Confirm the German translation is pasted at the cursor and the original clipboard content is restored afterward (paste path saves/restores clipboard).

Expected: translated text appears; no crash; `handy.log` shows `ClipboardTranslateAction::start` and a successful paste. Empty-clipboard and image-only-clipboard presses are silent no-ops (check the debug log line).

- [ ] **Step 4: Verify the shortcut appears and is rebindable in Settings**

Open Settings → Translation section. Confirm a "Перевод буфера обмена" shortcut row shows `Ctrl+Alt+Shift+Space` and can be rebound without error.

---

## Self-Review

**Spec coverage:**
- "Translate text currently in memory (clipboard)" → Task 1 Step 5 reads `clipboard().read_text()`. ✅
- "On a keyboard shortcut" → Task 1 Steps 6–7 register action + default binding; routed via handler.rs simple path. ✅
- Reuse existing translation pipeline (no dup) → Task 1 uses `build_translation_prompt` + `run_llm`. ✅
- Shared target language → Task 1 uses `settings.translation_target_language`. ✅
- Output pasted at cursor → Task 1 Step 5 uses `utils::paste`. ✅
- Discoverable/rebindable in UI → Task 2. ✅
- Works for the existing user's already-written settings file → relies on `get_settings` binding merge (settings.rs:986–993), no extra task needed. ✅

**Placeholder scan:** No TBD/TODO/"handle edge cases" — empty-clipboard, read error, empty-LLM-output, and paste error are each handled explicitly in Step 5's code.

**Type consistency:** `translate_clipboard` id is identical across `ACTION_MAP`, the default binding, `ShortcutInput shortcutId`, and both i18n key paths. `run_llm` / `build_translation_prompt` / `utils::paste` signatures match their definitions in actions.rs and clipboard.rs. `read_text()` returns `Result<String, _>` — handled with `match`.

**Note (not a blocker):** `translate_clipboard` is intentionally excluded from `is_transcribe_binding`, so it uses the `is_pressed` start / release stop path in `handler.rs`. `start` spawns the async work; `stop` is a no-op — correct for a one-shot.
