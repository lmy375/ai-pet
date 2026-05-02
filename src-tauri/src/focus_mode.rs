//! macOS Focus / Do-Not-Disturb detection.
//!
//! When the user has any Focus mode engaged (Personal, Work, Sleep, Driving, custom...),
//! macOS writes active assertions to
//! `~/Library/DoNotDisturb/DB/Assertions.json`. The pet uses this signal to stay quiet —
//! a Focus mode is the strongest possible "please don't bother me right now".
//!
//! Returns `None` (unknown) on non-macOS, when the file is absent (no third-party Focus
//! state has ever been written), or when we lack permission to read it. Callers should
//! treat None as "don't gate" so behavior degrades to other guards rather than locking up.

#[cfg(target_os = "macos")]
pub async fn focus_mode_active() -> Option<bool> {
    let home = std::env::var("HOME").ok()?;
    let path = std::path::PathBuf::from(home)
        .join("Library/DoNotDisturb/DB/Assertions.json");
    let bytes = tokio::fs::read(&path).await.ok()?;
    let val: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    // The `data` array holds active assertions while a Focus is engaged. Empty (or absent)
    // means no Focus is on — `Some(false)` so the caller knows we successfully checked.
    let active = val
        .get("data")
        .and_then(|d| d.as_array())
        .map(|arr| !arr.is_empty())
        .unwrap_or(false);
    Some(active)
}

#[cfg(not(target_os = "macos"))]
pub async fn focus_mode_active() -> Option<bool> {
    None
}
