//! Thin Tauri wrappers over `pet_core::settings`. Settings-mutating commands
//! emit `settings-changed` after the write — each window holds its own
//! in-memory settings copy and reloads on that event (without it the pet window
//! wouldn't react to a panel-side change, e.g. enabling gallery mode, until
//! refocused). Keep the emit on any new settings-writing command.

use pet_core::settings::{self, AgentConfig, AppSettings, McpServerConfig, ModelConfig};
use tauri::Emitter;

fn emit_settings_changed(app: &tauri::AppHandle) {
    let _ = app.emit("settings-changed", ());
}

#[tauri::command]
pub fn get_settings() -> Result<AppSettings, String> {
    settings::get_settings()
}

#[tauri::command]
pub fn save_settings(app: tauri::AppHandle, settings: AppSettings) -> Result<(), String> {
    settings::save_settings(&settings)?;
    emit_settings_changed(&app);
    Ok(())
}

/// Templates for the settings UI's "add" buttons. Default VALUES live on this
/// side (pet-core's `Default` impls) — the UI keeps no copy that could drift.
#[tauri::command]
pub fn new_agent(id: String, name: String) -> AgentConfig {
    AgentConfig { id, name, ..Default::default() }
}

#[tauri::command]
pub fn new_model() -> ModelConfig {
    // The wire model id is the user's pick and starts empty; the `gpt-4o-mini`
    // default only covers fields missing from a hand-edited config file.
    ModelConfig { model: String::new(), ..Default::default() }
}

#[tauri::command]
pub fn new_mcp_server() -> McpServerConfig {
    McpServerConfig::default()
}

/// Switch the active agent (the one answering the desktop chat window).
#[tauri::command]
pub fn set_active_agent(app: tauri::AppHandle, id: String) -> Result<(), String> {
    settings::set_active_agent(&id)?;
    emit_settings_changed(&app);
    Ok(())
}

/// Change one agent's `model` — used by the in-chat model switcher.
#[tauri::command]
pub fn set_agent_model(app: tauri::AppHandle, id: String, model: String) -> Result<(), String> {
    settings::set_agent_model(&id, &model)?;
    emit_settings_changed(&app);
    Ok(())
}

#[tauri::command]
pub fn get_config_raw() -> Result<String, String> {
    settings::get_config_raw()
}

#[tauri::command]
pub fn save_config_raw(app: tauri::AppHandle, content: String) -> Result<(), String> {
    settings::save_config_raw(&content)?;
    emit_settings_changed(&app);
    Ok(())
}

/// The selectable provider (wire-protocol) options, and what "Auto" would
/// resolve to for the given model. Served from Rust so `provider::PROVIDERS`
/// stays the single source of truth — a hardcoded copy in the UI would drift
/// the moment a provider is added.
#[tauri::command]
pub fn list_providers(model: String, provider: String) -> ProviderOptions {
    ProviderOptions {
        options: pet_core::provider::PROVIDERS
            .iter()
            .map(|(id, label)| ProviderOption {
                id: id.to_string(),
                label: label.to_string(),
            })
            .collect(),
        resolved: pet_core::provider::resolved_id(&provider, &model).to_string(),
        renders_budget: pet_core::provider::renders_reasoning_budget(
            pet_core::provider::kind(&provider, &model),
        ),
    }
}

#[derive(serde::Serialize)]
pub struct ProviderOption {
    pub id: String,
    pub label: String,
}

#[derive(serde::Serialize)]
pub struct ProviderOptions {
    pub options: Vec<ProviderOption>,
    /// The provider id actually used for a request with this config — equal to
    /// `provider` unless it's empty, in which case it's genai's inference.
    pub resolved: String,
    /// Whether this protocol can express a numeric thinking budget. False for
    /// the OpenAI protocols, where a budget would be silently discarded.
    pub renders_budget: bool,
}

#[tauri::command]
pub async fn list_models(
    api_base: String,
    api_key: String,
    provider: String,
    model: String,
) -> Result<Vec<String>, String> {
    settings::list_models(api_base, api_key, provider, model).await
}

#[tauri::command]
pub async fn test_model(
    api_base: String,
    api_key: String,
    model: String,
    provider: String,
) -> Result<(), String> {
    settings::test_model(api_base, api_key, model, provider).await
}

/// The default Live2D directory: `<config>/live2d`. Used as the starting point
/// for the file pickers when choosing an external core or model — the owner
/// drops files here instead of rebuilding the app.
#[tauri::command]
pub fn default_live2d_dir() -> Result<String, String> {
    let dir = settings::ensure_config_dir()?.join("live2d");
    Ok(dir.to_string_lossy().to_string())
}

/// Download Live2D's official sample SDK + wanko model and return the two paths
/// to configure. Rerunning re-downloads and overwrites, so the button doubles as
/// a repair for broken or missing sample files. The download/extract logic lives
/// in `pet_core::live2d`; see `docs/release.md` for the copyright note.
#[tauri::command]
pub async fn download_example_live2d() -> Result<pet_core::live2d::ExampleLive2DPaths, String> {
    pet_core::live2d::install_example().await
}
