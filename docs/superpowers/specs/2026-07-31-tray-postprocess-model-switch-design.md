# Tray Post-Processing Model Switcher — Design

**Date:** 2026-07-31
**Status:** Approved (design), pending spec review

## Goal

Let the user switch the post-processing / translation LLM model from the system tray, the same way the tray language switcher works — a submenu listing the installed models with a checkmark on the active one, one click to switch. Models are auto-discovered from the active provider (Ollama for the `custom` provider) and cached so the tray menu never blocks on the network.

## Scope & decisions (locked with user)

- **Auto-discovery** of models (not a manual list), cached in managed state; refreshed at startup and via a "Refresh models" item.
- **Active provider only.** The submenu switches `post_process_models[active_provider_id]` for whatever provider is currently selected (currently `custom`/Ollama). Switching the provider itself (OpenAI ↔ Ollama) is out of scope.
- **Tray only.** No hotkey, no window, no model management (pull/delete).
- The cache read in `update_tray_menu` is synchronous and non-blocking; all network I/O happens in spawned async tasks.
- The active model is always shown and checked, even if the cache is empty or stale (same "append current" rule as the language switcher).

## Architecture

Backend only. Reuses `crate::llm_client::fetch_models` (async) for discovery and the existing tray/menu-event patterns.

### Component 1: model cache (managed state) — `tray.rs`

A managed struct mirroring the existing `CurrentTrayIconState` pattern:

```rust
pub struct PostProcessModelCache(std::sync::Mutex<Vec<String>>);

impl PostProcessModelCache {
    pub fn new() -> Self { Self(std::sync::Mutex::new(Vec::new())) }
    pub fn get(&self) -> Vec<String> { self.0.lock().unwrap().clone() }
    pub fn set(&self, models: Vec<String>) { *self.0.lock().unwrap() = models; }
}
```

Registered in `lib.rs` setup: `app_handle.manage(tray::PostProcessModelCache::new());` (next to `tray::CurrentTrayIconState::new()`).

### Component 2: async refresh helper — `tray.rs`

```rust
/// Fetch the active provider's model list and store it in the cache, then
/// rebuild the tray so the submenu reflects it. Network I/O runs off the
/// menu-build thread. On error the cache is left as-is.
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

(`fetch_models` skips no provider by key emptiness — for `custom` an empty key is fine, exactly as the existing `fetch_post_process_models` command relies on.)

### Component 3: startup population — `lib.rs`

After the tray is initialized and the first `update_tray_menu` runs, call `tray::refresh_post_process_models(app_handle.clone());` so the cache is populated shortly after launch.

### Component 4: the submenu — `tray.rs::update_tray_menu`

Built in the Idle menu, next to the existing `model_submenu` (transcription) and `language_submenu`:

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
        // Skip a blank current model (no model configured yet) so we don't render an empty row.
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
    let _ = submenu.append(&PredefinedMenuItem::separator(app).expect("separator"));
    let _ = submenu.append(&refresh);
    submenu
};
```

Added to the Idle `Menu::with_items` list after `&language_submenu`. Recording/Transcribing menus unchanged. When the active provider has no configured model and the cache is empty, the submenu shows only the "Refresh models" action.

### Component 5: shared pure helper — generalize `language_menu_entries` → `checked_entries`

`language_menu_entries(&[&str], &str)` and the model list (`Vec<String>`) share the same shape, so generalize to one helper and route both callers through it:

```rust
/// Ordered `(label, is_checked)` entries: one per item, plus `current` appended
/// (checked) if it is not already present, so the active value is always shown.
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

The language submenu switches its call from `language_menu_entries(TRANSLATION_LANGUAGE_SHORTLIST, &current_lang)` to `checked_entries(TRANSLATION_LANGUAGE_SHORTLIST, &current_lang)`; the two existing tests move to `checked_entries` (behavior identical). `language_menu_entries` is removed.

### Component 6: menu-event handlers — `lib.rs`

Two arms after the `translate_lang:` arm:

```rust
id if id.starts_with("pp_model_select:") => {
    let model = id.strip_prefix("pp_model_select:").unwrap().to_string();
    let mut settings = settings::get_settings(app);
    let provider_id = settings.post_process_provider_id.clone();
    if settings.post_process_models.get(&provider_id) == Some(&model) {
        return;
    }
    settings.post_process_models.insert(provider_id.clone(), model.clone());
    settings::write_settings(app, settings);
    let _ = app.emit(
        "settings-changed",
        serde_json::json!({ "setting": "post_process_models", "provider": provider_id, "value": model }),
    );
    log::info!("Post-process model switched to {} via tray.", model);
    tray::update_tray_menu(app, None);
}
"pp_model_refresh" => {
    tray::refresh_post_process_models(app.clone());
}
```

### Component 7: i18n — `tray` keys

Add to `src/i18n/locales/en/translation.json` `tray`:

```json
"postProcessModel": "Post-Processing Model",
"refreshModels": "Refresh models"
```

and `ru`:

```json
"postProcessModel": "Модель постобработки",
"refreshModels": "Обновить список"
```

`build.rs` generates `TrayStrings.post_process_model` and `.refresh_models`.

## Data flow

1. Startup → tray built (submenu shows current model only, from settings) → `refresh_post_process_models` spawned → cache filled → tray rebuilt with the full list.
2. Click a model → settings updated, tray rebuilt (checkmark moves) → the next post-processing/translation uses the new model.
3. Click "Refresh models" → re-fetch → cache updated → tray rebuilt (picks up newly `ollama pull`ed models).

## Error handling

- Ollama down / fetch error: `refresh_post_process_models` logs a warning, cache unchanged; the submenu still shows the current model + Refresh. No crash.
- Active provider `None`: `refresh_post_process_models` returns early; the submenu shows the current model (possibly empty → only Refresh).
- Blank current model: the empty entry is skipped so no empty row renders.

## Testing

- `checked_entries` unit tests (`tray.rs`): current-in-list (one check, no dup, correct length) and current-not-in-list (appended last, checked) — the two migrated language tests plus one exercising a `Vec<String>` model list with a not-in-list current.
- `PostProcessModelCache` get/set round-trip (trivial).
- Discovery + submenu build + event handlers: verified by the standalone build + manual tray test (switch model, confirm checkmark moves and a subsequent translation uses it; Refresh picks up a newly pulled model). `cargo test` execution remains blocked here (`0xc0000139`); correctness of the pure helper rests on compile + inspection.

## Out of scope (YAGNI)

- Switching the provider from the tray, model pull/delete, per-feature model overrides, showing model sizes/metadata, localizing model names.

## Files touched

- `src-tauri/src/tray.rs` — `PostProcessModelCache`, `refresh_post_process_models`, generalized `checked_entries` (replacing `language_menu_entries`), the submenu, tests.
- `src-tauri/src/lib.rs` — `manage(PostProcessModelCache)`, startup refresh call, two `on_menu_event` arms.
- `src/i18n/locales/en/translation.json`, `src/i18n/locales/ru/translation.json` — `tray.postProcessModel`, `tray.refreshModels`.
