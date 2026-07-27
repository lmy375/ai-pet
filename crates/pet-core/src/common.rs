//! Shared helpers used across command modules: config paths, timestamps,
//! and OpenAI-compatible HTTP request building.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// A shared `reqwest::Client` (cheap to clone — it's `Arc`-backed internally),
/// reused for all outbound HTTP so connections can be pooled.
pub fn http_client() -> reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(reqwest::Client::new).clone()
}

/// Base config directory for the app: `<os config dir>/pet`.
pub fn config_dir() -> Result<PathBuf, String> {
    let dir = dirs::config_dir().ok_or_else(|| "Cannot determine config directory".to_string())?;
    Ok(dir.join("pet"))
}

/// Ensure the parent directory of `path` exists, creating it (and any missing
/// ancestors) if needed. No-op when `path` has no parent.
pub fn ensure_parent_dir(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory {}: {e}", parent.display()))?;
    }
    Ok(())
}

/// Write `content` to `path`, creating parent dirs first. Error messages name
/// the file (its last path segment) so failures are easy to pinpoint.
pub fn write_text(path: &Path, content: &str) -> Result<(), String> {
    ensure_parent_dir(path)?;
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    std::fs::write(path, content).map_err(|e| format!("Failed to write {name}: {e}"))
}

/// Read `path` as a UTF-8 string, falling back to `default` if it can't be read
/// (missing file, permission error, invalid UTF-8).
pub fn read_or(path: &Path, default: &str) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|_| default.to_string())
}

/// ISO-8601 timestamp with millisecond precision (e.g. `2026-06-14T09:30:00.123`).
/// Used for sessions and LLM logs so the format is identical everywhere.
pub fn iso_now() -> String {
    chrono::Local::now().format("%Y-%m-%dT%H:%M:%S%.3f").to_string()
}

/// Build an OpenAI-compatible endpoint URL from a base and path, normalizing
/// surrounding whitespace and slashes (e.g. base `https://x/v1/`, path `models`).
pub fn openai_endpoint(base: &str, path: &str) -> String {
    format!("{}/{}", base.trim().trim_end_matches('/'), path.trim_start_matches('/'))
}

/// Attach a `Bearer` auth header when an API key is present (trimmed). Local
/// endpoints (e.g. Ollama) often run keyless, so an empty key sends no header.
pub fn with_bearer(req: reqwest::RequestBuilder, api_key: &str) -> reqwest::RequestBuilder {
    let key = api_key.trim();
    if key.is_empty() {
        req
    } else {
        req.header("Authorization", format!("Bearer {}", key))
    }
}
