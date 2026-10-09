use base64::Engine;

use crate::tools::{required_str, tool_error, Tool, ToolContext};

pub struct ReadImageTool;

impl Tool for ReadImageTool {
    fn name(&self) -> &str {
        "read_image"
    }

    fn definition(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "read_image",
                "description": "Load an image from a local file path and attach it so you can actually SEE it. Use this to look at an image the user points you at, or one you produced (a photo, a chart, a saved screenshot). The image is shown to you on the next turn; this result carries only its metadata (format and size). Supports png, jpeg, gif and webp.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "file_path": {
                            "type": "string",
                            "description": "Absolute path to the image file"
                        }
                    },
                    "required": ["file_path"]
                }
            }
        })
    }

    crate::impl_execute!(read_image_impl);
}

async fn read_image_impl(arguments: &str, ctx: &ToolContext) -> String {
    let args = super::parse_args(arguments);
    let file_path = match required_str(&args, "file_path") {
        Ok(v) => v,
        Err(e) => return e,
    };

    let bytes = match std::fs::read(&file_path) {
        Ok(b) => b,
        Err(e) => return tool_error(format!("failed to read file: {}", e)),
    };

    let Some(mime) = sniff_mime(&bytes) else {
        return tool_error(format!(
            "not a supported image (png, jpeg, gif, webp only): {}",
            file_path
        ));
    };

    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    ctx.emit_image(format!("data:{};base64,{}", mime, b64));

    ctx.log(&format!(
        "read_image: {} ({}, {} KB)",
        file_path,
        mime,
        bytes.len() / 1024
    ));

    serde_json::json!({
        "file_path": file_path,
        "status": "ok",
        "mime": mime,
        "size_bytes": bytes.len(),
        "note": "Image attached — it appears in the next message for you to view.",
    })
    .to_string()
}

/// Recognize the image type from its magic bytes (more reliable than the file
/// extension). Returns one of the MIME types the model APIs accept.
fn sniff_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        Some("image/png")
    } else if bytes.starts_with(&[0xFF, 0xD8]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}