//! Mute + transient_note user-control surface (R52 / R55 / GOAL 049).
//!
//! Tauri commands + pure formatters. Pure helpers (compute_new_mute_until /
//! compute_new_transient_note / MUTE_UNTIL / TRANSIENT_NOTE / TransientNote)
//! live in `proactive::gate` — this module is the thin command + pin-to-memory
//! wrapper layer.

use super::gate::{
    compute_new_mute_until, compute_new_transient_note, mute_remaining_seconds, MUTE_UNTIL,
    TRANSIENT_NOTE,
};

/// Iter R52: set transient mute for `minutes` from now. Used when user
/// wants pet quiet during a focused session without flipping the
/// permanent `proactive.enabled` setting. Pass 0 to clear. Returns the
/// resulting `MUTE_UNTIL` ISO timestamp (or empty when cleared) so the
/// frontend can show a confirmation chip / countdown.
#[tauri::command]
pub fn set_mute_minutes(minutes: i64) -> String {
    // R59: pure helper extracted; Tauri command is now thin wrapper that
    // computes new state + writes to mutex + formats response.
    let new_until = compute_new_mute_until(minutes, chrono::Local::now());
    if let Ok(mut g) = MUTE_UNTIL.lock() {
        *g = new_until;
    }
    // 记录"今日 engage mute"的次数：仅 minutes > 0 路径（即真正启用静默）
    // 进 counter；clear（minutes <= 0）不计。前端 ChatMini「🔕 今日 mute」
    // chip 读 mute_count::get_today_mute_count 显该计数。
    if minutes > 0 {
        crate::mute_count::record_mute_engaged();
    }
    new_until
        .map(|t| t.format("%Y-%m-%dT%H:%M:%S%:z").to_string())
        .unwrap_or_default()
}

/// Iter R52: read the current MUTE_UNTIL state. Returns ISO timestamp
/// when active, empty string when not muted (or expired). Stay
/// consistent with chip semantics — `mute_remaining_seconds()` returns
/// None for both "never muted" and "expired", so frontend treats both
/// as "not muted" without needing to distinguish.
#[tauri::command]
pub fn get_mute_until() -> String {
    let Some(secs) = mute_remaining_seconds() else {
        return String::new();
    };
    let until = chrono::Local::now() + chrono::Duration::seconds(secs);
    until.format("%Y-%m-%dT%H:%M:%S%:z").to_string()
}

/// Iter R55: set transient instruction note for `minutes` from now. Empty
/// text or 0 minutes clears. Distinct from mute (R52) — note doesn't
/// block proactive turns, just adds context. Returns ISO timestamp
/// when active, empty when cleared.
#[tauri::command]
pub fn set_transient_note(text: String, minutes: i64) -> String {
    // R59: pure helper extracted; Tauri command thin wrapper.
    let new_note = compute_new_transient_note(&text, minutes, chrono::Local::now());
    let until_iso = new_note
        .as_ref()
        .map(|n| n.until.format("%Y-%m-%dT%H:%M:%S%:z").to_string())
        .unwrap_or_default();
    if let Ok(mut g) = TRANSIENT_NOTE.lock() {
        *g = new_note;
    }
    until_iso
}

/// Pure：把一段 transient_note 文本转成 PanelMemory entry title——首
/// `TRANSIENT_PIN_TITLE_CHARS` 个 char（按 char 计而非 byte，中文安全）+
/// `…` 若被截。空 / 全空白返回兜底 title `"transient note"`。
///
/// title 不含 `[source: ...]` marker（marker 由 description 携带）。
pub fn format_transient_pin_title(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return "transient note".to_string();
    }
    let chars: Vec<char> = trimmed.chars().collect();
    if chars.len() <= TRANSIENT_PIN_TITLE_CHARS {
        return trimmed.to_string();
    }
    let mut s: String = chars.into_iter().take(TRANSIENT_PIN_TITLE_CHARS).collect();
    s.push('…');
    s
}

/// Pure：把一段 transient_note 文本转成 PanelMemory entry description——前
/// 缀 `[source: transient_note_pin]` marker（与 spec 「同 source 字段标记」
/// 对应；将来 audit / 重复落库防护用）。
pub fn format_transient_pin_description(text: &str) -> String {
    format!("[source: transient_note_pin] {}", text.trim())
}

/// title char cap — 短到能在 PanelMemory list 单行展示。中文 30 个 ≈ 英文 90 个。
pub const TRANSIENT_PIN_TITLE_CHARS: usize = 30;

/// 允许的 PanelMemory category 白名单——caller（GOAL 049 frontend / LLM
/// tool）传任意字符串时本表先验。命中 → 用 caller 选择；不命中 / None →
/// fallback 到 `ai_insights`（pet 学到的"用户处于何状态"自然归到此 cat）。
const TRANSIENT_PIN_ALLOWED_CATEGORIES: &[&str] = &[
    "ai_insights",
    "user_profile",
    "general",
    "todo",
];

/// GOAL 049：把当前 transient_note 落 PanelMemory 一条常规 entry 并清掉
/// transient_note。spec「与 TG /here_pin 完全对偶」——本路径是对称的"反
/// 向"：/here_pin 从 task views 拼一条 transient_note；本 cmd 从 transient
/// _note 反向落 memory。
///
/// 参数：
/// - `edited`: None → 用当前 transient_note 文本；Some(s) → 用 user 编辑后
///   文本（spec「编辑后记下」流）。空 / 全空白返 Err。
/// - `category`: None → 默认 "ai_insights"；非白名单值 → 退回 "ai_insights"
///   （不 panic）。
///
/// 返回：新落入 memory 的 entry title（前端展示「✓ 已记下『xxx』」用）。
///
/// 失败语义（spec「记不下，再试一次」）：memory_edit fail → Err 原样返；前端
/// 展示 retry。transient_note 在写盘成功后才清，避免"清掉了但没落库"。
///
/// **重复落库防护**：spec「不重复落两条」——若已存在同 title 的 entry 直接
/// 返 Ok(title) 不写第二条（memory_edit "create" 在 title 已存在时本身报
/// "已存在" Err，本 fn 把该 Err 视作"成功"语义返 Ok title）。
#[tauri::command]
pub fn pin_transient_note(
    edited: Option<String>,
    category: Option<String>,
) -> Result<String, String> {
    // 1. 取文本：edited 优先；否则当前 transient_note
    let source_text = match edited {
        Some(s) if !s.trim().is_empty() => s,
        Some(_) => return Err("编辑后文本为空".to_string()),
        None => {
            let (current, _) = get_transient_note();
            if current.trim().is_empty() {
                return Err("当前没有 transient_note 可记".to_string());
            }
            current
        }
    };
    // 2. 校 category（白名单）
    let cat = category
        .as_deref()
        .filter(|c| TRANSIENT_PIN_ALLOWED_CATEGORIES.contains(c))
        .unwrap_or("ai_insights")
        .to_string();
    // 3. 拼 title + description
    let title = format_transient_pin_title(&source_text);
    let description = format_transient_pin_description(&source_text);
    // 4. memory_edit create——失败 path 1：已存在同 title 视作 idempotent
    //    返同 title；其它 path 原样回报。
    match crate::commands::memory::memory_edit(
        "create".to_string(),
        cat.clone(),
        title.clone(),
        Some(description),
        None,
    ) {
        Ok(_) => {
            // 5. 仅在写盘成功后清 transient_note——spec「不重复落两条」前
            //    提是先有 entry 后才清掉来源。
            let _ = set_transient_note(String::new(), 0);
            Ok(title)
        }
        Err(e) => {
            // 同 title 已存在 → 当作 idempotent 成功；clear transient_note
            // 后返同 title。memory_edit 错误文本含「已存在」/「duplicate」
            // 关键词时识别。
            let lower = e.to_ascii_lowercase();
            if lower.contains("已存在") || lower.contains("duplicate") || lower.contains("exists") {
                let _ = set_transient_note(String::new(), 0);
                return Ok(title);
            }
            Err(e)
        }
    }
}

/// Iter R55: read current TRANSIENT_NOTE state. Returns `(text, until_iso)`
/// when active, both empty when none. Frontend uses both: text for chip
/// preview, until for countdown.
#[tauri::command]
pub fn get_transient_note() -> (String, String) {
    let Some(note) = TRANSIENT_NOTE.lock().ok().and_then(|g| g.clone()) else {
        return (String::new(), String::new());
    };
    let now = chrono::Local::now();
    if note.until <= now {
        return (String::new(), String::new());
    }
    (
        note.text,
        note.until.format("%Y-%m-%dT%H:%M:%S%:z").to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::{
        format_transient_pin_description, format_transient_pin_title, TRANSIENT_PIN_TITLE_CHARS,
    };

    #[test]
    fn title_uses_short_text_verbatim() {
        let s = format_transient_pin_title("短文本");
        assert_eq!(s, "短文本");
    }

    #[test]
    fn title_truncates_long_text_with_ellipsis() {
        let long: String = "记".repeat(TRANSIENT_PIN_TITLE_CHARS + 10);
        let s = format_transient_pin_title(&long);
        assert!(s.ends_with('…'));
        // 截后 char 数 = cap + 省略号 = cap + 1
        assert_eq!(s.chars().count(), TRANSIENT_PIN_TITLE_CHARS + 1);
    }

    #[test]
    fn title_trims_surrounding_whitespace_before_count() {
        let s = format_transient_pin_title("   有内容   ");
        assert_eq!(s, "有内容");
    }

    #[test]
    fn title_empty_falls_back_to_default() {
        assert_eq!(format_transient_pin_title(""), "transient note");
        assert_eq!(format_transient_pin_title("   "), "transient note");
    }

    #[test]
    fn title_handles_chinese_char_count_not_byte() {
        // 中文字符按 char 计——确保 cap 不会切到 byte 中间造成无效 UTF-8
        let s = format_transient_pin_title(&"中".repeat(50));
        // 切完应正好 cap 个中文 + …
        assert_eq!(s.chars().count(), TRANSIENT_PIN_TITLE_CHARS + 1);
        // 切割不破坏 UTF-8 边界（如果破坏 chars().count() 会读到 replacement char）
        assert!(s.chars().take(TRANSIENT_PIN_TITLE_CHARS).all(|c| c == '中'));
    }

    #[test]
    fn description_includes_source_marker() {
        let s = format_transient_pin_description("用户回来不久还在 slack 忙");
        // spec「同 source 字段标记」对应 — marker 在 description 起首
        assert!(s.starts_with("[source: transient_note_pin]"));
        assert!(s.contains("用户回来不久"));
    }

    #[test]
    fn description_trims_whitespace_before_marker_appended() {
        let s = format_transient_pin_description("  正文  ");
        // 检查 marker 后只有一个空格 + 正文，不留空白多余
        assert_eq!(s, "[source: transient_note_pin] 正文");
    }
}
