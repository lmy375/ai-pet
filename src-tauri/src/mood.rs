//! Pet mood/state — the LLM-managed feeling that persists across turns.
//!
//! 心情存储位置：`~/.config/pet/current_mood.txt`，单文件、纯文本（`[motion: X]
//! free text`）。**不再放在 memory index 里** —— 用户明确说心情不属于"记忆"，
//! 而且 PanelMemory 的删除/编辑入口对系统态心情是误导。LLM 通过 `memory_edit`
//! 在 `ai_insights/current_mood` 写入时，由 `commands::memory` 拦截转写到本
//! 文件（首次拦截会顺带把旧 memory 条目移走，迁移幂等）。
//!
//! 读取从文件来；如果文件不存在但旧 memory 条目还在（从未触发过迁移），
//! `read_current_mood` 会做一次 lazy migrate。所有 LLM entry point（proactive /
//! chat / telegram / consolidate）共用这套读 helper，行为对称。
//!
//! `MOOD_CATEGORY` / `MOOD_TITLE` 仍保留：拦截层用它们识别 LLM 的写入意图。

use std::path::PathBuf;
use std::sync::atomic::Ordering;

use crate::commands::debug::write_log;
use crate::commands::memory;
use crate::tools::ToolContext;

/// 拦截识别用：当 LLM 调 `memory_edit` 写到 ai_insights/current_mood 时，
/// 由 commands::memory 转写到 mood_state_path。
pub const MOOD_CATEGORY: &str = "ai_insights";
pub const MOOD_TITLE: &str = "current_mood";

/// 心情文件路径：`~/.config/pet/current_mood.txt`。失败时返回 None，调用方
/// 静默退化（不同于 memory_edit，心情写失败没有阻塞性影响）。
pub fn mood_state_path() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("pet").join("current_mood.txt"))
}

/// v9: kv_state 里 mood 用的固定 key。
const MOOD_KV_KEY: &str = "current_mood";

/// 把 raw 心情串（含可选 `[motion: X]` 前缀）双写到 SQLite kv_state +
/// 旧 current_mood.txt 文件（保留文件为回滚保险 + 用户偶尔直接看文件）。
/// 空串视为清空。文件写失败静默吞 —— SQLite 仍是新 source-of-truth。
pub fn record_current_mood(raw: &str) {
    let trimmed = raw.trim();
    // SQLite 优先写：未来的真相源。
    crate::db::kv_set(MOOD_KV_KEY, trimmed);
    // 文件继续双写：回滚 / 外部读保险。
    if let Some(p) = mood_state_path() {
        if let Some(parent) = p.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&p, trimmed);
    }
}

/// 删除心情：SQLite kv_state + 旧文件。LLM 极少需要清空；保留接口仅给
/// 拦截层（memory_edit delete 走过来）+ 单测使用。
pub fn clear_current_mood() {
    crate::db::kv_delete(MOOD_KV_KEY);
    if let Some(p) = mood_state_path() {
        let _ = std::fs::remove_file(&p);
    }
}

/// 读心情：v9 优先 SQLite kv_state；fallback 旧 current_mood.txt（让升级
/// 用户首次启动 + backfill 之前仍能读到）。空串视作未记录。
fn read_mood_file() -> Option<String> {
    if let Some(v) = crate::db::kv_get(MOOD_KV_KEY) {
        let trimmed = v.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    let p = mood_state_path()?;
    let s = std::fs::read_to_string(&p).ok()?;
    let trimmed = s.trim();
    if trimmed.is_empty() {
        None
    } else {
        // 一次性 backfill：文件有值但 kv 没有 → 写回 SQLite。
        crate::db::kv_set(MOOD_KV_KEY, trimmed);
        Some(trimmed.to_string())
    }
}

/// 一次性迁移：如果旧 memory 索引里还有 ai_insights/current_mood 条目，把
/// description 写到新文件 + 从 memory 里删除。已经在文件里则 noop。
fn migrate_mood_from_memory_if_needed() -> Option<String> {
    let index = memory::memory_list(Some(MOOD_CATEGORY.to_string())).ok()?;
    let cat = index.categories.get(MOOD_CATEGORY)?;
    let entry_desc = cat
        .items
        .iter()
        .find(|i| i.title == MOOD_TITLE)
        .map(|i| i.description.clone())?;
    record_current_mood(&entry_desc);
    let _ = memory::memory_edit(
        "delete".to_string(),
        MOOD_CATEGORY.to_string(),
        MOOD_TITLE.to_string(),
        None,
        None,
    );
    Some(entry_desc)
}

/// Iter Cο: serializable shape exposed to the panel via `get_current_mood`. Carries
/// both the parsed (text + motion) and the raw description for inspection. Empty
/// `text` and `None` motion together mean "no mood recorded yet" — frontend should
/// render an empty state rather than dashes.
#[derive(serde::Serialize)]
pub struct CurrentMood {
    pub text: String,
    pub motion: Option<String>,
    pub raw: String,
}

/// Tauri command — current mood for the panel persona view. Returns an all-empty
/// shape (raw="") when the entry hasn't been written yet so the frontend can
/// distinguish "no mood" from "mood with empty text".
#[tauri::command]
pub fn get_current_mood() -> CurrentMood {
    match read_current_mood() {
        Some(raw) => {
            let (text, motion) = parse_mood_string(&raw);
            CurrentMood { text, motion, raw }
        }
        None => CurrentMood {
            text: String::new(),
            motion: None,
            raw: String::new(),
        },
    }
}

/// Read the pet's current mood/state. 主路径：`mood_state_path()` 文件。
/// 文件缺失但旧版本写过 `ai_insights/current_mood` memory 条目时一次性迁
/// 移到文件并删除旧条目。两处都没有则返回 None。
pub fn read_current_mood() -> Option<String> {
    if let Some(s) = read_mood_file() {
        return Some(s);
    }
    migrate_mood_from_memory_if_needed()
}

/// Parse `current_mood` into (mood_text, motion_group). The LLM is instructed to write
/// descriptions in the form `[motion: X] free-form text` where X is one of the Live2D
/// motion group names. If the prefix is absent, motion is None and text is the raw value.
///
/// Returns None if no mood is recorded yet.
pub fn read_current_mood_parsed() -> Option<(String, Option<String>)> {
    let raw = read_current_mood()?;
    Some(parse_mood_string(&raw))
}

/// Pure-function variant of the parsing — extracted for unit testing without touching the
/// memory store. Splits an optional `[motion: X]` prefix off the raw description; if the
/// prefix is missing, malformed, or carries a too-long tag, falls back to the raw text
/// with motion=None. Returns owned strings so callers don't have to manage lifetimes
/// against the source.
pub fn parse_mood_string(raw: &str) -> (String, Option<String>) {
    let trimmed = raw.trim_start();
    if let Some(after_open) = trimmed.strip_prefix("[motion:") {
        if let Some(close_idx) = after_open.find(']') {
            let motion = after_open[..close_idx].trim().to_string();
            let text = after_open[close_idx + 1..].trim().to_string();
            // Defend against empty or impossibly long tags from a confused model.
            if !motion.is_empty() && motion.len() <= 16 {
                return (text, Some(motion));
            }
        }
    }
    (raw.to_string(), None)
}

/// Shared post-turn mood read used by every LLM entry point (proactive, chat, telegram,
/// consolidate). Reads the current mood, parses the optional `[motion: X]` prefix, emits
/// a compliance log line when the prefix is missing, and bumps the process-wide
/// `MoodTagCounters` so the panel can display a cumulative format-adherence ratio.
/// `source` is the human-readable label that prefixes the log line so the user can tell
/// which pipeline produced the warning.
pub fn read_mood_for_event(ctx: &ToolContext, source: &str) -> (Option<String>, Option<String>) {
    let parsed = read_current_mood_parsed();
    let counters = &ctx.process_counters.mood_tag;
    match &parsed {
        Some((text, None)) if !text.trim().is_empty() => {
            counters.without_tag.fetch_add(1, Ordering::Relaxed);
            write_log(
                &ctx.log_store.0,
                &format!(
                    "{}: mood missing [motion: X] prefix — frontend will fall back to keyword match",
                    source
                ),
            );
        }
        Some((_, Some(_))) => {
            counters.with_tag.fetch_add(1, Ordering::Relaxed);
        }
        Some((_, None)) => {
            // Mood was present but text was empty/whitespace — treat as no_mood for stats
            // since the model didn't really write anything.
            counters.no_mood.fetch_add(1, Ordering::Relaxed);
        }
        None => {
            counters.no_mood.fetch_add(1, Ordering::Relaxed);
        }
    }
    match parsed {
        Some((t, m)) => (Some(t), m),
        None => (None, None),
    }
}

#[cfg(test)]
mod tests {
    use super::parse_mood_string;

    #[test]
    fn parses_well_formed_prefix() {
        let (text, motion) = parse_mood_string("[motion: Tap] 看用户在写代码，替他高兴");
        assert_eq!(motion.as_deref(), Some("Tap"));
        assert_eq!(text, "看用户在写代码，替他高兴");
    }

    #[test]
    fn allows_extra_whitespace_inside_prefix() {
        let (text, motion) = parse_mood_string("[motion:   Flick3   ]   有点烦躁");
        assert_eq!(motion.as_deref(), Some("Flick3"));
        assert_eq!(text, "有点烦躁");
    }

    #[test]
    fn no_prefix_returns_raw_with_none() {
        let (text, motion) = parse_mood_string("觉得今天过得很平静");
        assert!(motion.is_none());
        assert_eq!(text, "觉得今天过得很平静");
    }

    #[test]
    fn empty_motion_falls_back() {
        let (text, motion) = parse_mood_string("[motion: ] 心情");
        assert!(motion.is_none());
        assert_eq!(text, "[motion: ] 心情");
    }

    #[test]
    fn oversized_motion_falls_back() {
        // 17 chars, exceeds 16 limit — defends against the LLM dumping prose into the slot.
        let (_text, motion) = parse_mood_string("[motion: aaaaaaaaaaaaaaaaa] hi");
        assert!(motion.is_none());
    }

    #[test]
    fn unclosed_bracket_falls_back() {
        let (text, motion) = parse_mood_string("[motion: Tap 心情没收尾");
        assert!(motion.is_none());
        assert_eq!(text, "[motion: Tap 心情没收尾");
    }

    #[test]
    fn empty_text_after_prefix() {
        let (text, motion) = parse_mood_string("[motion: Idle]");
        assert_eq!(motion.as_deref(), Some("Idle"));
        assert_eq!(text, "");
    }

    #[test]
    fn handles_leading_whitespace() {
        let (text, motion) = parse_mood_string("   [motion: Tap] hello");
        assert_eq!(motion.as_deref(), Some("Tap"));
        assert_eq!(text, "hello");
    }
}
