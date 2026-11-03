//! Live2D sample installer — the settings card's "use sample model" button.
//!
//! The Cubism SDK and models are copyrighted and gitignored (docs/release.md),
//! so a fresh clone or a release built without LIVE2D_ASSETS_URL ships with no
//! Live2D assets at all. Rather than a bundled fallback, the empty state shows a
//! notice in the pet window and this module fetches Live2D's official sample
//! assets on demand. Both downloads are © Live2D Inc. (sample data of
//! live2d.com); they land under `<config>/live2d/`, next to where the file
//! pickers start.

use std::path::{Path, PathBuf};

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

/// Download Live2D's official sample SDK + wanko model and return the two paths
/// to configure. Rerunning re-downloads and overwrites, so the button doubles as
/// a repair for broken or missing sample files.
pub async fn install_example() -> Result<ExampleLive2DPaths, String> {
    let base = crate::settings::ensure_config_dir()?.join("live2d");

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
    let resp = crate::common::http_client()
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
fn unpack_sample_model(zip_bytes: &[u8], target_dir: &Path) -> Result<PathBuf, String> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip_bytes))
        .map_err(|e| format!("Sample model zip: {e}"))?;
    std::fs::create_dir_all(target_dir)
        .map_err(|e| format!("Failed to create {}: {e}", target_dir.display()))?;

    let mut best: Option<(usize, PathBuf)> = None;
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
            if best.as_ref().is_none_or(|(d, _)| depth < *d) {
                best = Some((depth, out));
            }
        }
    }
    best.map(|(_, p)| p)
        .ok_or_else(|| "Sample model zip contains no .model3.json".to_string())
}