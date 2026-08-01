# Tray Post-Processing Model Switcher Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a system-tray submenu that switches the post-processing / translation LLM model (for the active provider), with auto-discovered models, a checkmark on the active one, and a "Refresh models" action.

**Architecture:** Backend-only. A managed `PostProcessModelCache` holds the model list; an async `refresh_post_process_models` populates it via the existing `llm_client::fetch_models` and rebuilds the tray; `update_tray_menu` reads the cache synchronously and builds the submenu; two `on_menu_event` arms in `lib.rs` handle select/refresh. The pure `language_menu_entries` helper is generalized to `checked_entries` and shared by both the language and model submenus.

**Tech Stack:** Rust (Tauri 2 tray `Submenu`/`CheckMenuItem`/`MenuItem`), `tauri::async_runtime`, i18n JSON → generated `TrayStrings` (via `src-tauri/build.rs`).

**Spec:** `docs/superpowers/specs/2026-07-31-tray-postprocess-model-switch-design.md`

## Global Constraints

- Push code ONLY to `origin` (2web/Handy-Mobile). NEVER to `upstream`.
- Windows build: `export CARGO_TARGET_DIR="C:/h"`. Standalone = `bun run tauri build --no-bundle` (bare `cargo build` = DEV-mode exe needing Vite). Stop the app first (`Stop-Process -Name handy -Force`). **Before building: kill ALL lingering `rustc`/`cargo`/`bun`/`link` processes** (orphans from killed build wrappers contend and cause `link.exe 0xc0000142`); launch ONE detached build and poll with short foreground loops — do NOT relaunch on a "killed" bash notification (the detached build keeps running).
- `cargo test` cannot execute here (`0xc0000139`, environmental). Verify pure logic by compile + inspection; verify glue by the standalone build + manual tray click.
- Switching affects `post_process_models[active_provider_id]` (the shared post-processing/translation model). No provider switching, no model management.
- Reuse `crate::llm_client::fetch_models`; do NOT block the tray-menu build on the network (cache read only).

---

## File Structure

- `src-tauri/src/tray.rs` — generalize `language_menu_entries` → `checked_entries`; add `PostProcessModelCache`, `refresh_post_process_models`, the `pp_model_submenu`; update tests.
- `src-tauri/src/lib.rs` — `manage(PostProcessModelCache)`, a startup `refresh_post_process_models` call, and two `on_menu_event` arms.
- `src/i18n/locales/en/translation.json`, `src/i18n/locales/ru/translation.json` — `tray.postProcessModel`, `tray.refreshModels`.

**Ordering constraint:** the submenu (Task 2) references `strings.post_process_model` / `strings.refresh_models` (generated from the en i18n keys) and `checked_entries` / `PostProcessModelCache` (Task 1). Task 1 adds i18n + the generalized helper + the cache struct and rewires the existing language submenu — all of which compile together. Task 2 adds the model submenu, refresh helper, startup wiring, and handlers.

---

### Task 1: Generalize the helper, add the cache struct, add i18n keys

**Files:**
- Modify: `src-tauri/src/tray.rs` (helper at line 26, call site at line 276, tests at lines 391–449; add cache struct)
- Modify: `src/i18n/locales/en/translation.json` (`tray` object), `src/i18n/locales/ru/translation.json` (`tray` object)

**Interfaces:**
- Produces: `pub(crate) fn checked_entries<S: AsRef<str>>(items: &[S], current: &str) -> Vec<(String, bool)>` (replaces `language_menu_entries`); `pub struct PostProcessModelCache` with `new()/get()/set(Vec<String>)`; `TrayStrings.post_process_model`, `TrayStrings.refresh_models`.

- [ ] **Step 1: Add the English i18n keys**

In `src/i18n/locales/en/translation.json`, in the `"tray"` object (which already has `translationLanguage`), add:

```json
"postProcessModel": "Post-Processing Model",
"refreshModels": "Refresh models"
```

- [ ] **Step 2: Add the Russian i18n keys**

In `src/i18n/locales/ru/translation.json`, in the `"tray"` object, add (valid JSON):

```json
"postProcessModel": "Модель постобработки",
"refreshModels": "Обновить список"
```

- [ ] **Step 3: Validate both JSON files parse**

Run: `node -e "require('./src/i18n/locales/en/translation.json'); require('./src/i18n/locales/ru/translation.json'); console.log('json ok')"`
Expected: `json ok`.

- [ ] **Step 4: Generalize the helper (tray.rs:26)**

Replace the existing helper:

```rust
pub(crate) fn language_menu_entries(shortlist: &[&str], current: &str) -> Vec<(String, bool)> {
    let mut entries: Vec<(String, bool)> =
        shortlist.iter().map(|c| (c.to_string(), *c == current)).collect();
    if !shortlist.iter().any(|c| *c == current) {
        entries.push((current.to_string(), true));
    }
    entries
}
```

with the generic version (keep the doc comment above it, updated):

```rust
/// Ordered `(label, is_checked)` entries: one per item, plus `current` appended
/// (checked) if it is not already present, so the active value is always shown.
/// Shared by the language and post-process-model tray submenus.
pub(crate) fn checked_entries<S: AsRef<str>>(items: &[S], current: &str) -> Vec<(String, bool)> {
    let mut entries: Vec<(String, bool)> = items
        .iter()
        .map(|s| (s.as_ref().to_string(), s.as_ref() == current))
        .collect();
    if !items.iter().any(|s| s.as_ref() == current) {
        entries.push((current.to_string(), true));
    }
    entries
}
```

- [ ] **Step 5: Update the language submenu call site (tray.rs:276)**

Change:
```rust
        for (code, is_active) in language_menu_entries(TRANSLATION_LANGUAGE_SHORTLIST, &current_lang) {
```
to:
```rust
        for (code, is_active) in checked_entries(TRANSLATION_LANGUAGE_SHORTLIST, &current_lang) {
```

- [ ] **Step 6: Update the tests (tray.rs:391–449)**

In the `mod tests` `use super::{ ... }` import (line 392–394), replace `language_menu_entries` with `checked_entries`. In the two tests, replace both `language_menu_entries(` calls with `checked_entries(`. Then add one model-flavored test to the same module:

```rust
    #[test]
    fn checked_entries_appends_current_model_when_absent() {
        let models = vec!["qwen2.5:3b".to_string(), "gemma4:12b".to_string()];
        let entries = checked_entries(&models, "llama3.2:3b");
        assert_eq!(entries.len(), 3);
        assert_eq!(entries.last(), Some(&("llama3.2:3b".to_string(), true)));
        assert_eq!(entries.iter().filter(|(_, c)| *c).count(), 1);
    }
```

- [ ] **Step 7: Add the cache struct (tray.rs)**

Near the top of `tray.rs` (after the `use` block / constants, before `update_tray_menu`), add:

```rust
/// In-memory cache of the active provider's available post-processing model
/// names, populated asynchronously so `update_tray_menu` never blocks on the
/// network. Managed state; mirrors the `CurrentTrayIconState` pattern.
pub struct PostProcessModelCache(std::sync::Mutex<Vec<String>>);

impl PostProcessModelCache {
    pub fn new() -> Self {
        Self(std::sync::Mutex::new(Vec::new()))
    }
    pub fn get(&self) -> Vec<String> {
        self.0.lock().unwrap().clone()
    }
    pub fn set(&self, models: Vec<String>) {
        *self.0.lock().unwrap() = models;
    }
}

impl Default for PostProcessModelCache {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 8: Commit**

```bash
git add src/i18n/locales/en/translation.json src/i18n/locales/ru/translation.json src-tauri/src/tray.rs
git commit -m "refactor(tray): generalize menu-entry helper; add pp-model cache + i18n"
```

---

### Task 2: Refresh helper, startup wiring, submenu, and handlers

**Files:**
- Modify: `src-tauri/src/tray.rs` (`refresh_post_process_models`; the `pp_model_submenu` in `update_tray_menu`; add it to the Idle menu)
- Modify: `src-tauri/src/lib.rs` (`manage(PostProcessModelCache)`; startup refresh call; two `on_menu_event` arms)

**Interfaces:**
- Consumes: `PostProcessModelCache`, `checked_entries` (Task 1), `strings.post_process_model` / `strings.refresh_models` (Task 1 i18n), `crate::llm_client::fetch_models`, `settings::{get_settings, write_settings}`, `settings.active_post_process_provider()`, `settings.post_process_provider_id`, `settings.post_process_models`, `settings.post_process_api_keys`.
- Produces: menu ids `pp_model_select:{model}` and `pp_model_refresh`; `pub fn refresh_post_process_models(app: AppHandle)`.

- [ ] **Step 1: Add the refresh helper (tray.rs)**

Add near `update_tray_menu` (it needs `AppHandle`, `Manager` — already imported):

```rust
/// Fetch the active provider's model list and store it in the cache, then
/// rebuild the tray. Network I/O runs off the menu-build thread; on error the
/// cache is left unchanged.
pub fn refresh_post_process_models(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let settings = settings::get_settings(&app);
        let Some(provider) = settings.active_post_process_provider().cloned() else {
            return;
        };
        let api_key = settings
            .post_process_api_keys
            .get(&provider.id)
            .cloned()
            .unwrap_or_default();
        match crate::llm_client::fetch_models(&provider, api_key).await {
            Ok(mut models) => {
                models.sort();
                app.state::<PostProcessModelCache>().set(models);
                update_tray_menu(&app, None);
            }
            Err(e) => log::warn!("Tray model refresh failed for '{}': {}", provider.id, e),
        }
    });
}
```

- [ ] **Step 2: Build the pp-model submenu (tray.rs::update_tray_menu)**

After the `language_submenu` block (and before `let unload_model_i = ...`), add:

```rust
let active_provider_id = settings.post_process_provider_id.clone();
let current_pp_model = settings
    .post_process_models
    .get(&active_provider_id)
    .cloned()
    .unwrap_or_default();
let cached_models = app.state::<PostProcessModelCache>().get();
let pp_model_submenu = {
    let submenu = Submenu::with_id(app, "pp_model_submenu", &strings.post_process_model, true)
        .expect("failed to create post-process model submenu");
    for (model, is_active) in checked_entries(&cached_models, &current_pp_model) {
        if model.is_empty() {
            continue;
        }
        let item_id = format!("pp_model_select:{}", model);
        let item = CheckMenuItem::with_id(app, &item_id, &model, true, is_active, None::<&str>)
            .expect("failed to create pp model item");
        let _ = submenu.append(&item);
    }
    let refresh = MenuItem::with_id(app, "pp_model_refresh", &strings.refresh_models, true, None::<&str>)
        .expect("failed to create refresh models item");
    let _ = submenu.append(&PredefinedMenuItem::separator(app).expect("failed to create separator"));
    let _ = submenu.append(&refresh);
    submenu
};
```

- [ ] **Step 3: Add the submenu to the Idle menu (tray.rs)**

In the `TrayIconState::Idle => Menu::with_items(...)` list, insert `&pp_model_submenu` right after `&language_submenu`:

```rust
                &language_submenu,
                &pp_model_submenu,
                &separator(),
```

Leave the Recording/Transcribing arm unchanged.

- [ ] **Step 4: Register the cache and populate at startup (lib.rs)**

Where the other managed state is registered (near `app_handle.manage(tray::CurrentTrayIconState::new());`), add:

```rust
app_handle.manage(tray::PostProcessModelCache::new());
```

After the tray is initialized and the first `utils::update_tray_menu(app_handle, None);` runs (search for that call in `setup`), add:

```rust
tray::refresh_post_process_models(app_handle.clone());
```

- [ ] **Step 5: Add the menu-event handlers (lib.rs)**

In the tray `.on_menu_event(|app, event| match event.id.as_ref() { ... })`, after the `translate_lang:` arm and before `_ => {}`, add:

```rust
            id if id.starts_with("pp_model_select:") => {
                let model = id.strip_prefix("pp_model_select:").unwrap().to_string();
                let mut settings = settings::get_settings(app);
                let provider_id = settings.post_process_provider_id.clone();
                if settings.post_process_models.get(&provider_id) == Some(&model) {
                    return;
                }
                settings
                    .post_process_models
                    .insert(provider_id.clone(), model.clone());
                settings::write_settings(app, settings);
                let _ = app.emit(
                    "settings-changed",
                    serde_json::json!({
                        "setting": "post_process_models",
                        "provider": provider_id,
                        "value": model
                    }),
                );
                log::info!("Post-process model switched to {} via tray.", model);
                tray::update_tray_menu(app, None);
            }
            "pp_model_refresh" => {
                tray::refresh_post_process_models(app.clone());
            }
```

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/tray.rs src-tauri/src/lib.rs
git commit -m "feat(tray): post-processing model submenu with auto-discovery + refresh"
```

---

### Task 3: Standalone build + manual verification

**Files:** none (build + verification only)

- [ ] **Step 1: Clean stray build processes, stop the app, build**

Run: `powershell -NoProfile -Command "Get-Process -Name rustc,cargo,bun,node,link,cmake -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue"`, then `powershell -NoProfile -Command "Stop-Process -Name handy -Force -ErrorAction SilentlyContinue"`, then launch ONE detached build: `export CARGO_TARGET_DIR="C:/h"; nohup bun run tauri build --no-bundle > C:/h/ppmodel-build.log 2>&1 &`. Poll `C:/h/ppmodel-build.log` with short foreground loops for `Built application` / `could not compile` / `link.exe.*failed`; do NOT relaunch. Expect `Finished release ... Built application at: C:\h\release\handy.exe` (~20–25 min). A real `error[Exxx]` with a `--> src` location is a code failure (fix-loop); a bare `could not compile` after a kill or a `link.exe 0xc0000142` is an environmental interruption (clean processes, retry once).

- [ ] **Step 2: Launch and confirm clean startup**

Run: `powershell -NoProfile -Command "Start-Process -FilePath 'C:/h/release/handy.exe' -WorkingDirectory 'C:/h/release'"`, wait ~13s. Check `C:/Users/Andre/AppData/Local/com.pais.handy/logs/handy.log` for `Shortcuts initialized successfully` and no `panic`. (The tray menu — including the new submenu, whose build uses `.expect()` — is built at startup; a no-panic launch proves it builds. The startup `refresh_post_process_models` should log nothing on success or a `Tray model refresh failed` warning if Ollama is down.)

- [ ] **Step 3: Manual tray verification**

1. Ensure Ollama is running (`curl -s http://localhost:11434/v1/models` lists models).
2. Open the tray → confirm a "Post-Processing Model" ("Модель постобработки") submenu listing the installed models (`qwen2.5:3b`, `gemma4:12b`, …) with a checkmark on the active one (`qwen2.5:3b`), plus a "Refresh models" ("Обновить список") item.
3. Click another model (e.g. `gemma4:12b`) → checkmark moves.
4. Copy text, press `Ctrl+Alt+Shift+Space` → the translation is produced by the newly selected model (confirm via `handy.log` `Starting LLM post-processing with provider 'custom' (model: gemma4:12b)`).
5. `ollama pull` a small model in a terminal, then click "Refresh models" → the new model appears in the submenu.

Expected: checkmark tracks selection; the chosen model is used; Refresh picks up newly pulled models; no errors in `handy.log`.

---

## Self-Review

**Spec coverage:**
- Auto-discovered models, cached, non-blocking tray read → Task 1 Step 7 (cache) + Task 2 Steps 1–2. ✅
- Active provider only; switch `post_process_models[provider]` → Task 2 Step 5. ✅
- Checkmark on active; current always shown → `checked_entries` (Task 1) + submenu (Task 2). ✅
- Startup population + "Refresh models" → Task 2 Steps 4 (startup) + 1/2/5 (refresh). ✅
- Localized submenu title + refresh label (en+ru) → Task 1 Steps 1–2. ✅
- Empty/blank/error handling (skip empty row; warn on fetch error; early-return on no provider) → Task 2 Steps 1–2. ✅
- DRY: one shared helper → Task 1 Steps 4–6. ✅
- settings-changed emit for UI sync → Task 2 Step 5 (note: like the language switcher, no frontend listener exists yet, so this does not live-update an open Settings window; the emit is harmless and consistent with the codebase — do not assert live-sync as delivered).

**Placeholder scan:** No TBD/TODO; every code step has concrete code and exact insertion points.

**Type consistency:** `pp_model_select:{model}` id identical in tray.rs (`format!`) and lib.rs (`strip_prefix`). `checked_entries<S: AsRef<str>>` consumed with `&[&str]` (language) and `&Vec<String>` (models) — both satisfy `S: AsRef<str>`. `PostProcessModelCache` `new/get/set` names consistent across definition (Task 1), `manage` (Task 2 Step 4), and `state::<PostProcessModelCache>()` reads (Task 2 Steps 1–2). `settings.post_process_models.get(&provider_id) == Some(&model)` compares `Option<&String>` on both sides. `strings.post_process_model` / `strings.refresh_models` match the generated snake_case of `postProcessModel` / `refreshModels`.

**Note (environmental):** `cargo test` cannot execute the harness here (`0xc0000139`); `checked_entries` correctness rests on compile + the three inline assertions; the glue is verified by the Task 3 standalone build + manual tray clicks.
