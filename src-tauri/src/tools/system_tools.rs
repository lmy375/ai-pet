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
async fn get_active_window_impl(ctx: &ToolContext) -> String {
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

    let output = match tokio::process::Command::new("osascript")
        .arg("-e")
        .arg(SCRIPT)
        .output()
        .await
    {
        Ok(o) => o,
        Err(e) => return format!(r#"{{"error": "failed to run osascript: {}"}}"#, e),
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return serde_json::json!({
            "error": "osascript failed",
            "stderr": stderr,
            "hint": "If this mentions accessibility, grant the pet app Accessibility permission in System Settings → Privacy & Security."
        })
        .to_string();
    }

    let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let (app_name, window_title) = match raw.split_once('|') {
        Some((a, w)) => (a.trim().to_string(), w.trim().to_string()),
        None => (raw.clone(), String::new()),
    };

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
async fn get_active_window_impl(_ctx: &ToolContext) -> String {
    serde_json::json!({
        "error": "get_active_window is only implemented on macOS",
    })
    .to_string()
}
