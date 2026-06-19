use serde::Serialize;
use std::path::Path;

/// One media file discovered in the gallery directory. `kind` is "image" or
/// "video"; `path` is the absolute filesystem path (the frontend turns it into a
/// loadable URL via `convertFileSrc`).
#[derive(Debug, Clone, Serialize)]
pub struct MediaItem {
    pub path: String,
    pub kind: String,
}

const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "gif", "webp", "bmp", "avif"];
const VIDEO_EXTS: &[&str] = &["mp4", "webm", "mov", "m4v", "ogv"];

fn classify(ext: &str) -> Option<&'static str> {
    let ext = ext.to_ascii_lowercase();
    if IMAGE_EXTS.contains(&ext.as_str()) {
        Some("image")
    } else if VIDEO_EXTS.contains(&ext.as_str()) {
        Some("video")
    } else {
        None
    }
}

/// The OS "Pictures" folder, used as the default gallery directory. `None` if it
/// can't be resolved.
#[tauri::command]
pub fn default_gallery_dir() -> Option<String> {
    dirs::picture_dir().map(|p| p.to_string_lossy().to_string())
}

/// List image/video files directly inside `dir`, sorted by file name. Returns an
/// error if the directory can't be read.
#[tauri::command]
pub fn list_gallery_media(dir: String) -> Result<Vec<MediaItem>, String> {
    let path = Path::new(&dir);
    if !path.is_dir() {
        return Err(format!("不是有效目录: {}", dir));
    }
    let entries = std::fs::read_dir(path).map_err(|e| format!("读取目录失败: {}", e))?;

    let mut items: Vec<MediaItem> = Vec::new();
    for entry in entries.flatten() {
        let p = entry.path();
        if !p.is_file() {
            continue;
        }
        let kind = match p.extension().and_then(|e| e.to_str()).and_then(classify) {
            Some(k) => k,
            None => continue,
        };
        items.push(MediaItem {
            path: p.to_string_lossy().to_string(),
            kind: kind.to_string(),
        });
    }
    items.sort_by(|a, b| a.path.to_lowercase().cmp(&b.path.to_lowercase()));
    Ok(items)
}
