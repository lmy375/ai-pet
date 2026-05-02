//! Calendar awareness via macOS Calendar.app + AppleScript.
//!
//! Reads upcoming events in a sliding window so the pet can drop in things like
//! "you've got that meeting in 20 minutes — are you ready?". First call will
//! prompt the user to grant Calendar access; subsequent calls reuse it.
//!
//! Limitations:
//! - macOS-only; non-macOS returns an error.
//! - Calendar.app launches in the background to serve the query; first call may
//!   take a few seconds.
//! - Only Calendar.app's accounts are visible. Outlook/Google calendars not
//!   subscribed in macOS Calendar won't appear.

use crate::tools::{Tool, ToolContext};

const DEFAULT_HOURS: u32 = 24;
const MAX_HOURS: u32 = 168; // one week
const MAX_EVENTS_RETURNED: usize = 20;

pub struct GetUpcomingEventsTool;

impl Tool for GetUpcomingEventsTool {
    fn name(&self) -> &str {
        "get_upcoming_events"
    }

    fn definition(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "get_upcoming_events",
                "description": "Get the user's upcoming calendar events in the next N hours from macOS Calendar.app. Useful for drop-ins like 'you have a meeting in 20 minutes' or 'busy day tomorrow huh'. First call may show a system permission prompt and take a few seconds while Calendar.app starts. Returns at most 20 events. Treat events as private — never read out the full list verbatim, just reference what's relevant.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "hours_ahead": {
                            "type": "integer",
                            "description": "Time window in hours from now. Default 24, max 168 (one week)."
                        }
                    },
                    "required": []
                }
            }
        })
    }

    fn execute<'a>(
        &'a self,
        arguments: &'a str,
        ctx: &'a ToolContext,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = String> + Send + 'a>> {
        Box::pin(get_upcoming_events_impl(arguments, ctx))
    }
}

#[cfg(target_os = "macos")]
async fn get_upcoming_events_impl(arguments: &str, ctx: &ToolContext) -> String {
    let args: serde_json::Value = serde_json::from_str(arguments).unwrap_or_default();
    let hours_ahead = args["hours_ahead"]
        .as_u64()
        .map(|h| h as u32)
        .unwrap_or(DEFAULT_HOURS)
        .clamp(1, MAX_HOURS);

    // Each event is one line, fields separated by TAB. Tabs in titles are vanishingly rare,
    // and unlike unicode separators we can be confident osascript preserves them.
    const SCRIPT: &str = r#"on run argv
    set hwindow to (item 1 of argv) as integer
    set tStart to current date
    set tEnd to tStart + (hwindow * hours)
    set acc to ""
    tell application "Calendar"
        repeat with c in calendars
            set cName to name of c
            try
                set evs to (every event of c whose start date >= tStart and start date <= tEnd)
                repeat with e in evs
                    set s to start date of e
                    set en to end date of e
                    set sumr to summary of e
                    set loc to ""
                    try
                        set rawLoc to location of e
                        if rawLoc is not missing value then set loc to rawLoc
                    end try
                    set sStr to (year of s) & "-" & (month of s as integer) & "-" & (day of s) & " " & (hours of s) & ":" & (minutes of s)
                    set eStr to (year of en) & "-" & (month of en as integer) & "-" & (day of en) & " " & (hours of en) & ":" & (minutes of en)
                    set acc to acc & sumr & tab & sStr & tab & eStr & tab & cName & tab & loc & linefeed
                end repeat
            end try
        end repeat
    end tell
    return acc
end run
"#;

    let output = match tokio::process::Command::new("osascript")
        .arg("-e")
        .arg(SCRIPT)
        .arg(hours_ahead.to_string())
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
            "hint": "If Calendar prompts for permission, allow it in System Settings → Privacy & Security → Calendars. If Calendar.app isn't running, the first call may take a few seconds.",
        })
        .to_string();
    }

    let raw = String::from_utf8_lossy(&output.stdout);
    let mut events = Vec::new();
    for line in raw.lines() {
        let line = line.trim_end_matches('\r');
        if line.trim().is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.splitn(5, '\t').collect();
        if parts.len() < 4 {
            continue;
        }
        events.push(serde_json::json!({
            "title": parts[0],
            "start": parts[1],
            "end": parts[2],
            "calendar": parts[3],
            "location": parts.get(4).copied().unwrap_or(""),
        }));
        if events.len() >= MAX_EVENTS_RETURNED {
            break;
        }
    }

    ctx.log(&format!(
        "get_upcoming_events: window={}h, events={}",
        hours_ahead,
        events.len()
    ));

    serde_json::json!({
        "events": events,
        "window_hours": hours_ahead,
        "count": events.len(),
        "truncated": events.len() == MAX_EVENTS_RETURNED,
    })
    .to_string()
}

#[cfg(not(target_os = "macos"))]
async fn get_upcoming_events_impl(_arguments: &str, _ctx: &ToolContext) -> String {
    serde_json::json!({
        "error": "get_upcoming_events is only implemented on macOS",
    })
    .to_string()
}
