use base64::Engine;

use crate::tools::{Tool, ToolContext};

/// Cap the long edge of the captured image. 1568px is Anthropic's recommended
/// max for vision input — beyond it the model downscales anyway, so resizing
/// here keeps on-screen text legible while cutting payload/token cost sharply.
const MAX_EDGE: u32 = 1568;

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
                "description": "Capture the user's current main screen and attach it for you to look at. Use this when you need to see what's happening on the user's display — what app/page they're on, an error on screen, a layout they're describing, etc. The captured image is shown to you on the next turn; there are no parameters.",
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
        arguments: &'a str,
        ctx: &'a ToolContext,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = String> + Send + 'a>> {
        Box::pin(screenshot_impl(arguments, ctx))
    }
}

async fn screenshot_impl(_arguments: &str, ctx: &ToolContext) -> String {
    let tmp_dir = std::env::temp_dir();
    let id = uuid::Uuid::new_v4().to_string();
    let png_path = tmp_dir.join(format!("pet-shot-{}.png", id));
    let jpg_path = tmp_dir.join(format!("pet-shot-{}.jpg", id));

    // Cleanup helper so we never leave temp files behind on any exit path.
    let cleanup = || {
        let _ = std::fs::remove_file(&png_path);
        let _ = std::fs::remove_file(&jpg_path);
    };

    // 1. Capture the main display. `-x` = silent (no shutter sound/UI).
    match std::process::Command::new("screencapture")
        .arg("-x")
        .arg(&png_path)
        .output()
    {
        Ok(out) if out.status.success() && png_path.exists() => {}
        Ok(out) => {
            cleanup();
            let stderr = String::from_utf8_lossy(&out.stderr);
            return serde_json::json!({
                "error": format!("screencapture failed: {}. The app likely needs Screen Recording permission — grant it in System Settings > Privacy & Security > Screen Recording, then restart the app.", stderr.trim()),
            })
            .to_string();
        }
        Err(e) => {
            cleanup();
            return serde_json::json!({
                "error": format!("failed to run screencapture: {}", e),
            })
            .to_string();
        }
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

    ctx.log(&format!("screenshot: captured ({} KB)", jpg_bytes.len() / 1024));
    ctx.emit_image(data_url);

    serde_json::json!({
        "status": "ok",
        "note": "Screenshot of the main screen captured and attached — it appears in the next message for you to view.",
    })
    .to_string()
}
