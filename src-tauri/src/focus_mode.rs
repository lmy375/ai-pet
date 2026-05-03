//! macOS Focus / Do-Not-Disturb detection.
//!
//! When the user has any Focus mode engaged (Personal, Work, Sleep, Driving, custom...),
//! macOS writes active assertions to
//! `~/Library/DoNotDisturb/DB/Assertions.json`. The pet uses this signal to (a) optionally
//! gate proactive turns and (b) inject the focus name into the LLM prompt so the pet's
//! conversation can reference the user's current state.
//!
//! Returns `None` (unknown) on non-macOS, when the file is absent (no third-party Focus
//! state has ever been written), or when we lack permission to read it. Callers should
//! treat None as "don't gate" so behavior degrades to other guards rather than locking up.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusStatus {
    pub active: bool,
    /// Friendly name of the active focus mode (e.g. "work", "personal", "sleep").
    /// `None` when no focus is active or the identifier couldn't be extracted.
    pub name: Option<String>,
}

/// Read the macOS focus state file and return parsed status. Returns `None` for any IO or
/// parse failure so callers can treat the gate as a no-op rather than crash.
#[cfg(target_os = "macos")]
pub async fn focus_status() -> Option<FocusStatus> {
    let home = std::env::var("HOME").ok()?;
    let path = std::path::PathBuf::from(home).join("Library/DoNotDisturb/DB/Assertions.json");
    let bytes = tokio::fs::read(&path).await.ok()?;
    let val: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    Some(parse_focus_status(&val))
}

#[cfg(not(target_os = "macos"))]
pub async fn focus_status() -> Option<FocusStatus> {
    None
}

/// Backward-compat shorthand for the gate code that only cares about active/inactive.
pub async fn focus_mode_active() -> Option<bool> {
    focus_status().await.map(|s| s.active)
}

/// Pure parser — extracts (active, name) from a parsed Assertions.json value. Layered
/// `and_then` chain because every depth in macOS's structure could be missing on different
/// macOS versions; we fail soft rather than panic on any unexpected shape.
///
/// The mode identifier macOS writes is `com.apple.donotdisturb.mode.<NAME>`; we slice off
/// everything before the last dot.
pub fn parse_focus_status(val: &serde_json::Value) -> FocusStatus {
    let data = val.get("data").and_then(|d| d.as_array());
    let active = data.map(|arr| !arr.is_empty()).unwrap_or(false);
    let name = data
        .and_then(|arr| arr.first())
        .and_then(|item| item.get("storeAssertionRecords"))
        .and_then(|records| records.as_array())
        .and_then(|arr| arr.first())
        .and_then(|rec| rec.get("assertionDetails"))
        .and_then(|d| d.get("assertionDetailsModeIdentifier"))
        .and_then(|v| v.as_str())
        .and_then(|s| s.rsplit('.').next().map(String::from))
        .filter(|s| !s.is_empty());
    FocusStatus { active, name }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn empty_data_means_inactive_no_name() {
        let v = json!({"data": []});
        let s = parse_focus_status(&v);
        assert!(!s.active);
        assert!(s.name.is_none());
    }

    #[test]
    fn missing_data_means_inactive() {
        let v = json!({});
        let s = parse_focus_status(&v);
        assert!(!s.active);
        assert!(s.name.is_none());
    }

    #[test]
    fn extracts_name_from_mode_identifier() {
        let v = json!({
            "data": [{
                "storeAssertionRecords": [{
                    "assertionDetails": {
                        "assertionDetailsModeIdentifier": "com.apple.donotdisturb.mode.work"
                    }
                }]
            }]
        });
        let s = parse_focus_status(&v);
        assert!(s.active);
        assert_eq!(s.name.as_deref(), Some("work"));
    }

    #[test]
    fn active_without_identifier_keeps_name_none() {
        let v = json!({"data": [{"storeAssertionRecords": []}]});
        let s = parse_focus_status(&v);
        assert!(s.active, "data array non-empty so active");
        assert!(s.name.is_none(), "no identifier to extract from");
    }

    #[test]
    fn handles_unexpected_shape_gracefully() {
        let v = json!({"data": "wrong-type"});
        let s = parse_focus_status(&v);
        assert!(!s.active, "non-array data should fail-soft to inactive");
        assert!(s.name.is_none());
    }

    #[test]
    fn identifier_without_dots_returns_full_string() {
        let v = json!({
            "data": [{
                "storeAssertionRecords": [{
                    "assertionDetails": {
                        "assertionDetailsModeIdentifier": "custom"
                    }
                }]
            }]
        });
        let s = parse_focus_status(&v);
        assert_eq!(s.name.as_deref(), Some("custom"));
    }
}
