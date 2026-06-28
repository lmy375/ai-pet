use base64::Engine;
use std::path::Path;
use std::process::Command;

use crate::tools::{Tool, ToolContext};

/// Cap the long edge of the captured image. 1568px is Anthropic's recommended
/// max for vision input — beyond it the model downscales anyway, so resizing
/// here keeps on-screen text legible while cutting payload/token cost sharply.
const MAX_EDGE: u32 = 1568;

/// JXA (osascript) that prints the on-screen window ids of an app, largest
/// first, space-separated. argv[0] is matched case-insensitively as a substring
/// against each window's owner name (so "chrome" matches "Google Chrome").
/// Only normal app windows (layer 0) above a minimum size are considered, so
/// menu-bar items / helper overlays are skipped. `1|16` =
/// kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements.
const WINDOW_IDS_JXA: &str = r#"function run(argv){
  ObjC.import("CoreGraphics"); ObjC.import("Foundation");
  var target=(argv[0]||"").toLowerCase();
  var arr=ObjC.deepUnwrap(ObjC.castRefToObject($.CGWindowListCopyWindowInfo(1|16,0)));
  var m=arr.filter(function(w){
    return w.kCGWindowLayer===0
      && (w.kCGWindowOwnerName||"").toLowerCase().indexOf(target)!==-1
      && w.kCGWindowBounds && w.kCGWindowBounds.Width>50 && w.kCGWindowBounds.Height>50;});
  m.sort(function(a,b){
    return (b.kCGWindowBounds.Width*b.kCGWindowBounds.Height)
         - (a.kCGWindowBounds.Width*a.kCGWindowBounds.Height);});
  return m.map(function(w){return w.kCGWindowNumber;}).join(" ");
}"#;

pub struct ScreenshotTool;

impl Tool for ScreenshotTool {
    fn name(&self) -> &str {
        "screenshot"
    }

    fn definition(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "screenshot",
                "description": "Capture the user's screen and attach it for you to look at. Use this when you need to see what's on the user's display — what app/page they're on, an error on screen, a layout they're describing, etc. By default captures the whole main screen. Pass `app` to capture just one application's window (e.g. the user asks you to look at their WeChat / Chrome). The image is shown to you on the next turn.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "app": {
                            "type": "string",
                            "description": "Optional. Capture only this application's window instead of the whole screen. Case-insensitive substring of the app name, e.g. \"WeChat\", \"微信\", \"Chrome\". The app must be running with a visible (non-minimized) window. Omit to capture the entire main screen."
                        }
                    },
                    "required": []
                }
            }
        })
    }

    crate::impl_execute!(screenshot_impl);
}

async fn screenshot_impl(arguments: &str, ctx: &ToolContext) -> String {
    // Optional `app` arg: capture just that application's window.
    let app = serde_json::from_str::<serde_json::Value>(arguments)
        .ok()
        .and_then(|v| v["app"].as_str().map(|s| s.trim().to_string()))
        .filter(|s| !s.is_empty());

    let tmp_dir = std::env::temp_dir();
    let id = uuid::Uuid::new_v4().to_string();
    let png_path = tmp_dir.join(format!("pet-shot-{}.png", id));
    let jpg_path = tmp_dir.join(format!("pet-shot-{}.jpg", id));

    // Cleanup helper so we never leave temp files behind on any exit path.
    let cleanup = || {
        let _ = std::fs::remove_file(&png_path);
        let _ = std::fs::remove_file(&jpg_path);
    };

    // 1. Capture to png_path — either one app window or the whole main display.
    let capture_result = match &app {
        Some(name) => capture_app_window(name, &png_path),
        None => capture_main_screen(&png_path),
    };
    if let Err(e) = capture_result {
        cleanup();
        return serde_json::json!({ "error": e }).to_string();
    }

    // 2. Downscale (long edge -> MAX_EDGE) and convert to JPEG via sips (built-in).
    let jpg_bytes = match std::process::Command::new("sips")
        .arg("--resampleHeightWidthMax")
        .arg(MAX_EDGE.to_string())
        .arg(&png_path)
        .arg("--out")
        .arg(&jpg_path)
        .arg("-s")
        .arg("format")
        .arg("jpeg")
        .output()
    {
        Ok(out) if out.status.success() && jpg_path.exists() => {
            match std::fs::read(&jpg_path) {
                Ok(b) => b,
                Err(e) => {
                    cleanup();
                    return serde_json::json!({ "error": format!("failed to read resized image: {}", e) }).to_string();
                }
            }
        }
        // sips failed — fall back to the raw PNG so we still return something usable.
        _ => match std::fs::read(&png_path) {
            Ok(b) => b,
            Err(e) => {
                cleanup();
                return serde_json::json!({ "error": format!("failed to read screenshot: {}", e) }).to_string();
            }
        },
    };

    let is_jpeg = jpg_path.exists();
    let mime = if is_jpeg { "image/jpeg" } else { "image/png" };
    let b64 = base64::engine::general_purpose::STANDARD.encode(&jpg_bytes);
    let data_url = format!("data:{};base64,{}", mime, b64);

    cleanup();

    let subject = match &app {
        Some(name) => format!("{}'s window", name),
        None => "the main screen".to_string(),
    };
    ctx.log(&format!("screenshot: captured {} ({} KB)", subject, jpg_bytes.len() / 1024));
    ctx.emit_image(data_url);

    serde_json::json!({
        "status": "ok",
        "note": format!("Screenshot of {} captured and attached — it appears in the next message for you to view.", subject),
    })
    .to_string()
}

/// Capture the whole main display to `png_path`. `-x` = silent (no shutter
/// sound/UI).
fn capture_main_screen(png_path: &Path) -> Result<(), String> {
    match Command::new("screencapture").arg("-x").arg(png_path).output() {
        Ok(out) if out.status.success() && png_path.exists() => Ok(()),
        Ok(out) => Err(format!(
            "screencapture failed: {}. The app likely needs Screen Recording permission — grant it in System Settings > Privacy & Security > Screen Recording, then restart the app.",
            String::from_utf8_lossy(&out.stderr).trim()
        )),
        Err(e) => Err(format!("failed to run screencapture: {}", e)),
    }
}

/// Capture a single app's window to `png_path`. Tries each matching on-screen
/// window (largest first); if none can be captured the app may be minimized or
/// on another Space, so we bring it to the front (`open -a`) and retry once.
fn capture_app_window(app: &str, png_path: &Path) -> Result<(), String> {
    if try_capture_first(&app_window_ids(app), png_path) {
        return Ok(());
    }

    // Fallback: surface a hidden/minimized/other-Space window, then retry.
    let _ = Command::new("open").arg("-a").arg(app).output();
    std::thread::sleep(std::time::Duration::from_millis(700));
    if try_capture_first(&app_window_ids(app), png_path) {
        return Ok(());
    }

    Err(format!(
        "couldn't capture a window for \"{}\". Make sure the app is running with a visible (non-minimized) window, and that this app has Screen Recording permission in System Settings > Privacy & Security.",
        app
    ))
}

/// On-screen window ids for `app` (largest first) via the JXA helper.
fn app_window_ids(app: &str) -> Vec<String> {
    match Command::new("osascript")
        .arg("-l")
        .arg("JavaScript")
        .arg("-e")
        .arg(WINDOW_IDS_JXA)
        .arg(app)
        .output()
    {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout)
            .split_whitespace()
            .map(|s| s.to_string())
            .collect(),
        _ => vec![],
    }
}

/// Try to capture each window id in turn; returns true on the first success.
/// `screencapture -l<id>` fails (non-zero) for off-screen/uncapturable windows,
/// so we fall through to the next candidate. `-o` omits the window's shadow.
fn try_capture_first(ids: &[String], png_path: &Path) -> bool {
    for id in ids {
        let _ = std::fs::remove_file(png_path); // avoid a stale file passing the check
        let ok = matches!(
            Command::new("screencapture")
                .arg("-x")
                .arg("-o")
                .arg(format!("-l{}", id))
                .arg(png_path)
                .output(),
            Ok(out) if out.status.success()
        );
        if ok && std::fs::metadata(png_path).map(|m| m.len() > 0).unwrap_or(false) {
            return true;
        }
    }
    false
}
