//! Detect how long the user has been idle from keyboard/mouse input.
//!
//! On macOS we shell out to `ioreg -c IOHIDSystem` and parse the `HIDIdleTime`
//! property (nanoseconds since the last HID event). This is the same value
//! `CGEventSourceSecondsSinceLastEventType` reports, but doesn't require a
//! native FFI dependency and works without Accessibility permission.
//!
//! Used by the proactive loop to avoid speaking up while the user is actively
//! typing or moving the mouse.

/// Returns seconds since the last keyboard/mouse event, or `None` if the value
/// can't be read (non-macOS, ioreg failure, parse failure).
pub async fn user_input_idle_seconds() -> Option<u64> {
    #[cfg(target_os = "macos")]
    {
        macos::user_input_idle_seconds().await
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

#[cfg(target_os = "macos")]
mod macos {
    pub async fn user_input_idle_seconds() -> Option<u64> {
        let output = tokio::process::Command::new("ioreg")
            .args(["-c", "IOHIDSystem"])
            .output()
            .await
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            // Looking for: `    | | |   "HIDIdleTime" = 60042178833`
            if let Some(rest) = line.split_once("\"HIDIdleTime\"") {
                let value = rest.1.trim_start_matches(|c: char| c == ' ' || c == '=');
                let nanos: u64 = value.trim().parse().ok()?;
                return Some(nanos / 1_000_000_000);
            }
        }
        None
    }
}
