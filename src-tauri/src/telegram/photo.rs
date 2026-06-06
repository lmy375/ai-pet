//! TG 图片输入的下载 / 缩放 / base64 编码流水。bot.rs 在收到 photo
//! message（或 media_group 聚合完成）后调进来，拿到可直接塞进 Anthropic
//! vision content `source.data` 的 base64 字符串。
//!
//! 关键约束（来自 GOAL.md）：
//! - 「大图按模型上限缩放后再送，保宽高比」→ 长边压到 [`MAX_DIMENSION`]
//!   像素，等比缩放；小图直接透传不放大。
//! - 「history 中不保存图片二进制」→ 本文件只做 in-flight 处理，调用方
//!   负责把 user 消息写进 session 时改成 `[图片]` 占位（见 bot.rs）。

use std::collections::HashMap;
use std::io::Cursor;

use base64::Engine;
use image::codecs::jpeg::JpegEncoder;
use image::ImageReader;
use teloxide::net::Download;
use teloxide::prelude::*;
use teloxide::types::ChatId;
use tokio::sync::Mutex as TokioMutex;

/// 缩放目标长边。1568px 是 Anthropic 文档推荐的「视觉效果 / token 成本」
/// 折中点：再大不会提升识别质量，但 token 数量线性上升。短边按宽高比缩。
const MAX_DIMENSION: u32 = 1568;

/// JPEG 重编码质量。85 在「肉眼无损」与「码率」之间是经验最优；TG 上传
/// 本来就是 JPEG，再压一次损失可忽略。
const JPEG_QUALITY: u8 = 85;

/// album 聚合的去抖时长。TG 把同一 media_group 的多张图作为独立 update
/// 推送，但同一 group 的全部更新一般在几百 ms 内到齐；1500ms 安全 buffer
/// 足够覆盖 99% 网络抖动，又不会让用户等太久才看到 bot 回复。
pub const ALBUM_DEBOUNCE_MS: u64 = 1500;

/// 单次 album 在 buffer 中的累积状态。media_group_id → 此结构。
pub struct AlbumPending {
    pub chat_id: ChatId,
    /// (file_id, 对应消息的 caption)。TG 通常只在第一条消息带 caption，
    /// 但为稳健起见保留每条；聚合时取第一个非空 caption。teloxide 0.13
    /// 里 file_id 是裸 `String`（0.14+ 才有 `FileId` 新类型）。
    pub photos: Vec<(String, Option<String>)>,
}

/// album 去抖 buffer 的全局句柄。`HashMap<media_group_id, AlbumPending>`。
pub type AlbumBuffer = TokioMutex<HashMap<String, AlbumPending>>;

/// 下载 TG 文件，缩放到 [`MAX_DIMENSION`] 长边以内，再 JPEG 编码并
/// base64。返回值可直接塞进 Anthropic vision content 的 `source.data`。
pub async fn download_and_prepare(bot: &Bot, file_id: &str) -> Result<String, String> {
    let file = bot
        .get_file(file_id.to_string())
        .await
        .map_err(|e| format!("get_file failed: {}", e))?;

    let mut raw: Vec<u8> = Vec::new();
    bot.download_file(&file.path, &mut raw)
        .await
        .map_err(|e| format!("download_file failed: {}", e))?;

    // 解码 + Lanczos3 缩放 + JPEG 编码是 CPU bound，单图量级数十 ms，整个
    // album 可叠到几百 ms。直接在 async fn 里跑会占住 tokio worker —— 用
    // spawn_blocking 隔离到专用线程池，避免阻塞其它聊天 / 命令 handler。
    let jpeg = tokio::task::spawn_blocking(move || resize_and_encode_jpeg(&raw))
        .await
        .map_err(|e| format!("resize task panicked: {}", e))??;
    Ok(base64::engine::general_purpose::STANDARD.encode(&jpeg))
}

/// 解码任意已启用 feature 的格式（JPEG / PNG），长边压到 [`MAX_DIMENSION`]，
/// 再以 JPEG 重编码。短边按宽高比缩，不放大。多模态 input path 默认入口。
pub fn resize_and_encode_jpeg(raw: &[u8]) -> Result<Vec<u8>, String> {
    resize_and_encode_jpeg_to(raw, MAX_DIMENSION)
}

/// 共用底层：把图片解码 → 长边压到 `max_dim` → JPEG 重编码。
/// `resize_and_encode_jpeg` 用 1568（多模态 LLM 输入），`visual_memory`
/// 用 200（PanelMemory 缩略图）。其它 caller 拿到 `max_dim` 自由决定。
pub fn resize_and_encode_jpeg_to(raw: &[u8], max_dim: u32) -> Result<Vec<u8>, String> {
    // guess_format 通过 magic bytes 嗅探，IO 量与 raw[0..16] 同量级；用它
    // 取代显式格式参数，让函数 TG path（JPEG）与桌面截屏 path（PNG）共用。
    let img = ImageReader::new(Cursor::new(raw))
        .with_guessed_format()
        .map_err(|e| format!("guess image format failed: {}", e))?
        .decode()
        .map_err(|e| format!("decode image failed: {}", e))?;

    let (w, h) = (img.width(), img.height());
    let scaled = if w.max(h) > max_dim {
        // image::imageops::resize 是 CPU bound 但单图 <2560px 量级耗时
        // 可控（数十 ms）。同步执行不阻塞 tokio runtime（reqwest 下载
        // 是大头）。Lanczos3 在缩小场景下质量最好。
        let (nw, nh) = scaled_dimensions(w, h, max_dim);
        img.resize_exact(nw, nh, image::imageops::FilterType::Lanczos3)
    } else {
        img
    };

    let mut out: Vec<u8> = Vec::new();
    let rgb = scaled.to_rgb8();
    let mut encoder = JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY);
    encoder
        .encode(
            rgb.as_raw(),
            rgb.width(),
            rgb.height(),
            image::ExtendedColorType::Rgb8,
        )
        .map_err(|e| format!("encode jpeg failed: {}", e))?;
    Ok(out)
}

/// 等比把 (w, h) 长边缩到 `target`，短边按宽高比保留至少 1px。
/// 拆出来便于单测「保宽高比」这条 GOAL 硬约束。
fn scaled_dimensions(w: u32, h: u32, target: u32) -> (u32, u32) {
    if w >= h {
        let nh = ((h as u64) * (target as u64) / (w as u64)).max(1) as u32;
        (target, nh)
    } else {
        let nw = ((w as u64) * (target as u64) / (h as u64)).max(1) as u32;
        (nw, target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaled_dimensions_preserves_aspect_ratio_landscape() {
        // 3200×1600，长边压到 1568：短边应为 784，宽高比 2:1 保留。
        let (w, h) = scaled_dimensions(3200, 1600, 1568);
        assert_eq!(w, 1568);
        assert_eq!(h, 784);
    }

    #[test]
    fn scaled_dimensions_preserves_aspect_ratio_portrait() {
        // 1600×3200，长边在 h：h 压到 1568，w 应为 784。
        let (w, h) = scaled_dimensions(1600, 3200, 1568);
        assert_eq!(w, 784);
        assert_eq!(h, 1568);
    }

    #[test]
    fn scaled_dimensions_extreme_aspect_no_zero_side() {
        // 10000×10 的极端横条：短边算出的 nh 会下溢到 0，必须 clamp 到 1
        // 否则 image crate 会 panic / 拒绝。这条 case 防回归。
        let (w, h) = scaled_dimensions(10000, 10, 1568);
        assert_eq!(w, 1568);
        assert!(h >= 1, "short side must clamp to at least 1, got {}", h);
    }
}
