//! 视觉记忆 item（GOAL 009）：把用户「想保留」的图片落 PanelMemory。
//!
//! 与 GOAL 001 的契约不冲突：001 要求**对话 history** 不留二进制；本模
//! 块是用户主动 opt-in 的写入动作，对应路径独立 —— 缩略图存到 memories
//! 目录下 `attachments/`，memory item 的 description 用 `[visual: <rel>]
//! <caption>` 前缀，PanelMemory 读到时按 path 渲缩略图。
//!
//! 触发途径（按介入程度排）：
//! 1. 用户消息含「记一下 / 存一下 / 以后看 / /keep」等关键词 + 图片：
//!    chat / TG 路径检测后 fire-and-forget [`keep_image_as_memory`]。
//! 2. ChatMini / 前端显式按钮调 Tauri 命令 [`keep_visual_memory`]（接受
//!    data URL）—— 用户不必组关键词。
//!
//! 删除链路：memory_edit("delete", ...) 检测 description 是否带 `[visual:
//! ...]` 前缀，命中时同时 unlink 缩略图文件。

use std::path::PathBuf;

use base64::Engine;

/// 缩略图长边上限（px）。GOAL 写「短边 ≤ 200」；这里实现为长边 ≤ 200
/// 以避免极端宽高比（如截屏 1×4000）退化成不可用缩略图。两者在多数实
/// 际场景行为等价；偏离仅在极端 aspect ratio 时，且偏向"更小的缩略图"
/// 总是 PanelMemory 渲染期望的方向。
const THUMB_MAX_DIM: u32 = 200;

/// 触发关键词。命中 substring 即视作「用户想保留这张图」。中英文混合，
/// `/keep` 显式优先放最前 —— 用户键入命令时不必依赖自然语言匹配。
pub const KEEP_INTENT_KEYWORDS: &[&str] = &[
    "/keep",
    "记一下",
    "存一下",
    "存下来",
    "以后看",
    "保存这张",
    "保存图片",
    "记下来",
    "save this",
    "keep this",
];

/// Pure：扫描 caption 文本，命中任一关键词返回 true。trim 后空字符串
/// 一律 false（无 caption 不触发）。
pub fn is_keep_intent(caption: &str) -> bool {
    let t = caption.trim();
    if t.is_empty() {
        return false;
    }
    let lower = t.to_lowercase();
    KEEP_INTENT_KEYWORDS
        .iter()
        .any(|k| lower.contains(&k.to_lowercase()))
}

/// 视觉条目 description 前缀正则。把 `[visual: <rel_path>] <rest>` 中的
/// `rel_path` 与剩余 caption / description 文本拆出，让 PanelMemory 端
/// 渲染缩略图、剩余文本走常规字段渲染。前端也有同款解析器（PanelMemory
/// 端 string match），保持两端协议一致 —— 这里是"权威"，前端是 mirror。
pub fn parse_visual_prefix(description: &str) -> Option<(String, String)> {
    let trimmed = description.trim_start();
    let after_open = trimmed.strip_prefix("[visual:")?;
    let close_idx = after_open.find(']')?;
    let rel = after_open[..close_idx].trim().to_string();
    if rel.is_empty() {
        return None;
    }
    let rest = after_open[close_idx + 1..].trim().to_string();
    Some((rel, rest))
}

/// 组装写入 memory description 的字段值。`llm_description` 可为空（GOAL
/// 要求"用户 caption + LLM 描述"，但 LLM 失败时退化到只用 caption）。
pub fn format_visual_description(
    thumb_rel_path: &str,
    caption: &str,
    llm_description: &str,
) -> String {
    let cap = caption.trim();
    let llm = llm_description.trim();
    let body = match (cap.is_empty(), llm.is_empty()) {
        (true, true) => "(无文字说明)".to_string(),
        (true, false) => llm.to_string(),
        (false, true) => cap.to_string(),
        (false, false) => format!("{} · {}", cap, llm),
    };
    format!("[visual: {}] {}", thumb_rel_path, body)
}

/// 把图片字节落到 memories/attachments/ 下；返回相对 memories_dir 的路
/// 径（如 `attachments/abc123def456.jpg`）。
///
/// filename 用 std DefaultHasher 出来的 16 hex char —— 不需要密码学强度，
/// 只要短 + 同输入同输出能去重（同图二次 keep 自动覆盖同文件）。注意
/// DefaultHasher 跨 Rust 版本不保证稳定，升级后旧 thumbnail 可能 orphan
/// —— 不阻塞功能，最多空间小量浪费。
///
/// 缩放调用 [`telegram::photo::resize_and_encode_jpeg_to`]（max_dim=200），
/// 复用已经验证的 image-crate 缩放路径。CPU 重活通过 spawn_blocking 避
/// 开 tokio worker 阻塞。
pub async fn save_thumbnail(raw_bytes: Vec<u8>) -> Result<String, String> {
    let mem_dir = crate::commands::memory::memories_dir()?;
    let att_dir = mem_dir.join("attachments");
    tokio::fs::create_dir_all(&att_dir)
        .await
        .map_err(|e| format!("无法创建 attachments 目录：{}", e))?;

    let jpeg = tokio::task::spawn_blocking(move || {
        crate::telegram::photo::resize_and_encode_jpeg_to(&raw_bytes, THUMB_MAX_DIM)
    })
    .await
    .map_err(|e| format!("thumbnail task panicked: {}", e))??;

    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    jpeg.hash(&mut hasher);
    let id = hasher.finish();
    let filename = format!("{:016x}.jpg", id);
    let rel = format!("attachments/{}", filename);
    let full = att_dir.join(&filename);
    tokio::fs::write(&full, &jpeg)
        .await
        .map_err(|e| format!("写入缩略图失败：{}", e))?;
    Ok(rel)
}

/// memory_edit 走 delete 时 hook：从被删 item 的 description 里抠出 visual
/// 前缀，把对应 attachments/ 文件清掉。失败 silently 吞（log）—— 主删除
/// 已成功，缩略图残留属于空间小问题。pub(crate) 让 memory.rs 直接调。
pub(crate) fn cleanup_thumbnail_on_delete(description: &str) {
    let (rel, _) = match parse_visual_prefix(description) {
        Some(v) => v,
        None => return,
    };
    let mem_dir = match crate::commands::memory::memories_dir() {
        Ok(d) => d,
        Err(_) => return,
    };
    // path-traversal 防御：拒绝包含 `..` 或绝对路径的 rel。
    if rel.contains("..") || PathBuf::from(&rel).is_absolute() {
        log::warn!("visual_memory: refused to delete suspicious thumb path: {}", rel);
        return;
    }
    let full = mem_dir.join(&rel);
    if let Err(e) = std::fs::remove_file(&full) {
        if e.kind() != std::io::ErrorKind::NotFound {
            log::warn!("visual_memory: failed to remove {:?}: {}", full, e);
        }
    }
}

/// Pure：从 caption 提取显式 `#tag` cat 命中（如 `#food` → 命中 `food`
/// 这个 category 名）。多个 tag 时取第一个。允许的 category 列表由
/// caller 传入（通常是 memory_list 现有 keys），避免创建新 cat —— GOAL
/// 「不创建新 cat」硬约束。无命中返回 None。
pub fn detect_explicit_category<'a>(
    caption: &str,
    available: &'a [String],
) -> Option<&'a String> {
    let re = regex::Regex::new(r"#([\w\-]+)").ok()?;
    for cap in re.captures_iter(caption) {
        let tag = cap.get(1)?.as_str().to_lowercase();
        if let Some(found) = available
            .iter()
            .find(|c| c.to_lowercase() == tag)
        {
            return Some(found);
        }
    }
    None
}

/// Pure：根据 caption 生成 memory item title。规则：
/// - 取 caption 第一行 / 第一段（按句号或换行切）；
/// - 截到 24 字符；
/// - 去前后空白；
/// - 空 → 「视觉记忆 YYYY-MM-DD HH:MM」时间戳兜底，保证非空 + 唯一性。
pub fn derive_title(caption: &str, now: chrono::DateTime<chrono::Local>) -> String {
    let trimmed = caption.trim();
    if trimmed.is_empty() {
        return now.format("视觉记忆 %Y-%m-%d %H:%M").to_string();
    }
    let first_line = trimmed
        .split(['\n', '。', '!', '?', '！', '？'])
        .next()
        .unwrap_or(trimmed)
        .trim();
    if first_line.is_empty() {
        return now.format("视觉记忆 %Y-%m-%d %H:%M").to_string();
    }
    let head: String = first_line.chars().take(24).collect();
    head
}

/// 一站式入口：保存缩略图 + 创建 memory item。LLM 描述生成委托给 caller
/// 异步预算（chat / TG 路径有 AppHandle + AiConfig），本函数不再额外调
/// LLM，避免双重路径。caller 传 `llm_description = ""` 时 description
/// 只用 caption。
///
/// `category` 缺省 `ai_insights`（与既有 cat 列表兼容；不创建新 cat）。
/// caller 已在外面用 [`detect_explicit_category`] 检过 `#tag` 命中时把
/// 命中值传进来。
pub async fn keep_image_as_memory(
    image_bytes: Vec<u8>,
    caption: &str,
    llm_description: &str,
    category: &str,
) -> Result<String, String> {
    let rel_path = save_thumbnail(image_bytes).await?;
    let description = format_visual_description(&rel_path, caption, llm_description);
    let title = derive_title(caption, chrono::Local::now());

    crate::commands::memory::memory_edit(
        "create".to_string(),
        category.to_string(),
        title.clone(),
        Some(description),
        None,
    )?;
    Ok(title)
}

/// Tauri 命令：前端把图片 data URL + caption 传过来，落地后返回 memory
/// item title。data URL 形如 `data:image/jpeg;base64,...`；缺前缀按裸
/// base64 兜底，让显式 `/keep` 调用更轻量。
#[tauri::command]
pub async fn keep_visual_memory(
    image_data_url: String,
    caption: String,
) -> Result<String, String> {
    let bytes = decode_data_url(&image_data_url)?;
    // category 选择：优先 caption 里的 `#tag`，否则 ai_insights。
    let index = crate::commands::memory::memory_list(None)?;
    let cats: Vec<String> = index.categories.keys().cloned().collect();
    let explicit = detect_explicit_category(&caption, &cats);
    let category = explicit
        .map(|s| s.as_str())
        .unwrap_or("ai_insights");
    // LLM 描述本轮不生成（避免一次显式 keep 触发额外 LLM cost；GOAL「LLM
    // 一句描述」改由用户在 caption 自带或后续手动 update。前端 chat path
    // 走 inject_keep_intent_hook 时若 LLM 上一轮已用 vision 看过图，可在
    // 它自己生成的话术里包含描述供用户复制 / refine。
    keep_image_as_memory(bytes, &caption, "", category).await
}

/// 前端 PanelMemory 渲染 visual item 缩略图时调本命令读 attachments/ 下
/// 的 jpeg → 返回 `data:image/jpeg;base64,...` data URL 直接塞 `<img src>`。
/// path-traversal 防御：rel_path 必须在 memories_dir/attachments/ 之下。
#[tauri::command]
pub fn read_attachment(rel_path: String) -> Result<String, String> {
    use std::fs;
    if rel_path.contains("..") || std::path::PathBuf::from(&rel_path).is_absolute() {
        return Err("非法路径".to_string());
    }
    let mem_dir = crate::commands::memory::memories_dir()?;
    let full = mem_dir.join(&rel_path);
    // 必须落在 attachments/ 子目录内：双重防御。
    let att_dir = mem_dir.join("attachments");
    if !full.starts_with(&att_dir) {
        return Err("路径未指向 attachments/".to_string());
    }
    let bytes = fs::read(&full).map_err(|e| format!("读取失败：{}", e))?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(format!("data:image/jpeg;base64,{}", b64))
}

fn decode_data_url(s: &str) -> Result<Vec<u8>, String> {
    let body = if let Some(comma) = s.find(',') {
        // 简单兜底：`data:image/...;base64,` 这种头部一律剥掉
        let header = &s[..comma];
        let body = &s[comma + 1..];
        if !header.to_lowercase().contains("base64") {
            return Err("仅支持 base64 编码的 data URL".to_string());
        }
        body
    } else {
        // 裸 base64 也接受
        s
    };
    base64::engine::general_purpose::STANDARD
        .decode(body)
        .map_err(|e| format!("base64 解码失败：{}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intent_detects_chinese_and_english_keywords() {
        assert!(is_keep_intent("记一下这张图"));
        assert!(is_keep_intent("save this please"));
        assert!(is_keep_intent("/keep"));
        assert!(is_keep_intent("以后看"));
        assert!(!is_keep_intent(""));
        assert!(!is_keep_intent("   "));
        assert!(!is_keep_intent("今天天气真好"));
    }

    #[test]
    fn parse_visual_prefix_extracts_path_and_rest() {
        let (rel, rest) = parse_visual_prefix("[visual: attachments/abc.jpg] 菜单照片").unwrap();
        assert_eq!(rel, "attachments/abc.jpg");
        assert_eq!(rest, "菜单照片");
    }

    #[test]
    fn parse_visual_prefix_rejects_empty_path() {
        assert!(parse_visual_prefix("[visual: ] hi").is_none());
        assert!(parse_visual_prefix("not visual").is_none());
    }

    #[test]
    fn format_visual_description_handles_missing_pieces() {
        assert_eq!(
            format_visual_description("attachments/x.jpg", "", ""),
            "[visual: attachments/x.jpg] (无文字说明)"
        );
        assert_eq!(
            format_visual_description("attachments/x.jpg", "菜单", ""),
            "[visual: attachments/x.jpg] 菜单"
        );
        assert_eq!(
            format_visual_description("attachments/x.jpg", "菜单", "牛肉面店"),
            "[visual: attachments/x.jpg] 菜单 · 牛肉面店"
        );
    }

    #[test]
    fn detect_explicit_category_matches_tag() {
        let cats = vec!["ai_insights".to_string(), "user_profile".to_string()];
        // 显式 #user_profile tag 命中
        let pick = detect_explicit_category("#user_profile 用户习惯偏好", &cats);
        assert_eq!(pick, Some(&"user_profile".to_string()));
        // 未命中 tag
        assert!(detect_explicit_category("#unknown_cat 测试", &cats).is_none());
        // 无 tag
        assert!(detect_explicit_category("普通 caption", &cats).is_none());
    }

    #[test]
    fn derive_title_truncates_long_caption() {
        let cap = "a".repeat(50);
        let now = chrono::Local::now();
        let title = derive_title(&cap, now);
        assert_eq!(title.chars().count(), 24);
    }

    #[test]
    fn derive_title_fallbacks_to_timestamp_for_empty() {
        let now = chrono::Local::now();
        let title = derive_title("", now);
        assert!(title.starts_with("视觉记忆 "));
    }

    #[test]
    fn derive_title_takes_first_sentence() {
        let now = chrono::Local::now();
        let title = derive_title("菜单照片。这家餐厅不错。", now);
        assert_eq!(title, "菜单照片");
    }
}
