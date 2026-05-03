//! Environment-awareness tools that let the pet "see" what the user is doing.
//! Currently macOS-only. The frontmost app is fetched via `osascript`; the front
//! window title requires Accessibility permission and may be empty for apps that
//! don't expose it (or before the user grants permission in System Settings).

use crate::tools::{Tool, ToolContext};

pub struct GetActiveWindowTool;

impl Tool for GetActiveWindowTool {
    fn name(&self) -> &str {
        "get_active_window"
    }

    fn definition(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "get_active_window",
                "description": "Get the user's currently frontmost application and window title on macOS. Use this to gauge what the user is doing right now (e.g. coding in VS Code, browsing in Safari, watching a video). Returns app name and window title; window title may be empty if the OS hasn't granted accessibility permission or the app doesn't expose it. Treat the result as a hint, not as authoritative — do not be overly specific about it when chatting with the user.",
                "parameters": {
                    "type": "object",
                    "properties": {},
                    "required": []
                }
            }
        })
    }

    fn execute<'a>(
        &'a self,
        _arguments: &'a str,
        ctx: &'a ToolContext,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = String> + Send + 'a>> {
        Box::pin(get_active_window_impl(ctx))
    }
}

#[cfg(target_os = "macos")]
/// Iter R15: thin Rust-only osascript wrapper exposing the current foreground
/// window for non-tool callers (proactive loop's active-app tracker). Returns
/// `(app_name, window_title)` raw — no redaction, no logging — so the caller
/// chooses whether to surface to LLM (redact first) or keep internal
/// (compare for transition tracking). None on osascript failure / non-macOS.
#[cfg(target_os = "macos")]
pub async fn current_active_window() -> Option<(String, String)> {
    const SCRIPT: &str = r#"
tell application "System Events"
    set frontApp to first application process whose frontmost is true
    set appName to name of frontApp
    set winName to ""
    try
        set winName to name of front window of frontApp
    end try
    return appName & "|" & winName
end tell
"#;
    let output = tokio::process::Command::new("osascript")
        .arg("-e")
        .arg(SCRIPT)
        .output()
        .await
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let (app, window) = match raw.split_once('|') {
        Some((a, w)) => (a.trim().to_string(), w.trim().to_string()),
        None => (raw, String::new()),
    };
    Some((app, window))
}

#[cfg(not(target_os = "macos"))]
pub async fn current_active_window() -> Option<(String, String)> {
    None
}

async fn get_active_window_impl(ctx: &ToolContext) -> String {
    #[cfg(target_os = "macos")]
    {
        let Some((app_name, window_title)) = current_active_window().await else {
            return serde_json::json!({
                "error": "osascript failed",
                "hint": "If this mentions accessibility, grant the pet app Accessibility permission in System Settings → Privacy & Security."
            })
            .to_string();
        };

        // Iter Cx: apply user-configured privacy redaction before either logging or
        // returning to the LLM. Both `app` and `window_title` can contain personal
        // names / project codenames; user lists patterns in settings.privacy.
        let patterns = crate::commands::settings::get_settings()
            .map(|s| s.privacy.redaction_patterns.clone())
            .unwrap_or_default();
        let app_name = crate::redaction::redact_text(&app_name, &patterns);
        let window_title = crate::redaction::redact_text(&window_title, &patterns);

        ctx.log(&format!(
            "get_active_window: app={:?} window={:?}",
            app_name, window_title
        ));

        serde_json::json!({
            "app": app_name,
            "window_title": window_title,
            "platform": "macos",
        })
        .to_string()
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = ctx;
        serde_json::json!({
            "error": "get_active_window is only implemented on macOS",
        })
        .to_string()
    }
}
