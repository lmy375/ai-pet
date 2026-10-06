//! Thin Tauri wrappers over `pet_core::settings`. Settings-mutating commands
//! emit `settings-changed` after the write — each window holds its own
//! in-memory settings copy and reloads on that event (without it the pet window
//! wouldn't react to a panel-side change, e.g. enabling gallery mode, until
//! refocused). Keep the emit on any new settings-writing command.

use pet_core::settings::{self, AppSettings};
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

#[tauri::command]
pub fn open_config_dir(app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let dir = settings::ensure_config_dir()?;
    app.opener()
        .open_path(dir.to_string_lossy().to_string(), None::<&str>)
        .map_err(|e| format!("Failed to open config dir: {}", e))
}

/// Open an arbitrary directory/file in the OS file manager (e.g. the gallery
/// folder in Finder). Used by the settings "open" buttons.
#[tauri::command]
pub fn open_path(app: tauri::AppHandle, path: String) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    if path.trim().is_empty() {
        return Err("路径为空".to_string());
    }
    app.opener()
        .open_path(path, None::<&str>)
        .map_err(|e| format!("Failed to open path: {}", e))
}

// ---------------------------------------------------------------------------
// Live2D sample installer — the settings card's "use sample model" button.
//
// The Cubism SDK and models are copyrighted and gitignored (docs/release.md),
// so a fresh clone or a release built without LIVE2D_ASSETS_URL ships with no
// Live2D assets at all. Rather than a bundled fallback, the empty state shows
// a notice in the pet window and this command fetches Live2D's official sample
// assets on demand. Both downloads are © Live2D Inc. (sample data of
// live2d.com); they land under `<config>/live2d/`, next to where the file
// pickers start.

const SAMPLE_CORE_URL: &str =
    "https://cubism.live2d.com/sdk-web/cubismcore/live2dcubismcore.min.js";
const SAMPLE_MODEL_URL: &str = "https://cubism.live2d.com/sample-data/bin/wanko/wanko_ja.zip";

#[derive(serde::Serialize)]
pub struct ExampleLive2DPaths {
    /// Absolute path to store in `live_2d_core_path`.
    pub core_path: String,
    /// Absolute path to store in `live_2d_model_path` (the extracted .model3.json).
    pub model_path: String,
}

/// Download Live2D's official sample SDK + wanko model and return the two
/// paths to configure. Rerunning re-downloads and overwrites, so the button
/// doubles as a repair for broken or missing sample files.
#[tauri::command]
pub async fn download_example_live2d() -> Result<ExampleLive2DPaths, String> {
    let base = settings::ensure_config_dir()?.join("live2d");

    // Core JS: a single file, stored as-is.
    let core_bytes = http_download(SAMPLE_CORE_URL).await?;
    let core_path = base.join("live2dcubismcore.min.js");
    std::fs::write(&core_path, &core_bytes)
        .map_err(|e| format!("Failed to write {}: {e}", core_path.display()))?;

    // Model: a zip. Unpack it whole — the .model3.json references textures and
    // motions by relative path — and point at the shallowest .model3.json.
    let zip_bytes = http_download(SAMPLE_MODEL_URL).await?;
    let model_dir = base.join("models").join("wanko");
    let model_path = unpack_sample_model(&zip_bytes, &model_dir)?;

    Ok(ExampleLive2DPaths {
        core_path: core_path.to_string_lossy().into_owned(),
        model_path: model_path.to_string_lossy().into_owned(),
    })
}

/// GET a URL and return the body. A browser-ish User-Agent, because CDNs
/// sometimes reject the bare reqwest one.
async fn http_download(url: &str) -> Result<Vec<u8>, String> {
    let resp = pet_core::common::http_client()
        .get(url)
        .header(
            "User-Agent",
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36",
        )
        .send()
        .await
        .map_err(|e| format!("GET {url}: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("GET {url}: HTTP {status}"));
    }
    resp.bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|e| format!("GET {url}: read body failed: {e}"))
}

/// Extract the sample zip into `target_dir` (zip-slip safe) and return the
/// shallowest `*.model3.json` it contained.
fn unpack_sample_model(
    zip_bytes: &[u8],
    target_dir: &std::path::Path,
) -> Result<std::path::PathBuf, String> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip_bytes))
        .map_err(|e| format!("Sample model zip: {e}"))?;
    std::fs::create_dir_all(target_dir)
        .map_err(|e| format!("Failed to create {}: {e}", target_dir.display()))?;

    let mut best: Option<(usize, std::path::PathBuf)> = None;
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| format!("Sample model zip: {e}"))?;
        // `enclosed_name` rejects absolute paths and `..` components (zip-slip).
        let Some(rel) = entry.enclosed_name() else { continue };
        let out = target_dir.join(&rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out)
                .map_err(|e| format!("Failed to create {}: {e}", out.display()))?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create {}: {e}", parent.display()))?;
        }
        let mut file = std::fs::File::create(&out)
            .map_err(|e| format!("Failed to write {}: {e}", out.display()))?;
        std::io::copy(&mut entry, &mut file)
            .map_err(|e| format!("Failed to write {}: {e}", out.display()))?;

        if rel.to_string_lossy().ends_with(".model3.json") {
            let depth = rel.components().count();
            if best.as_ref().map_or(true, |(d, _)| depth < *d) {
                best = Some((depth, out));
            }
        }
    }
    best.map(|(_, p)| p)
        .ok_or_else(|| "Sample model zip contains no .model3.json".to_string())
}
