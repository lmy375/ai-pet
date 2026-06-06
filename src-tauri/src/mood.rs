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

/// GOAL 047：mood-tag → emoji 映射表。常量集中可扩展。按特异度排序——
/// 更具体的 mood 关键词放前面，让混合表达（"开心但有点累"）命中第一项
/// "累"→😴，与 017 classify_mood_policy「混合时 Postpone 优先」spirit 不
/// 完全同：本表为视觉 cue，强情绪信号优先于平淡积极信号。
///
/// 关键词 case-insensitive 子串匹配；首条命中即返。
pub const MOOD_EMOJI_TABLE: &[(&str, &str)] = &[
    // 高优先级——强负面 / 强信号情绪
    ("崩溃", "😭"),
    ("沮丧", "😞"),
    ("低落", "😞"),
    ("难过", "😢"),
    ("伤心", "😢"),
    ("焦虑", "😟"),
    ("担心", "😟"),
    ("不安", "😟"),
    ("烦躁", "😤"),
    ("焦躁", "😤"),
    ("生气", "😤"),
    ("愤怒", "😡"),
    // 中等优先级——疲惫 / 思考状态（用户场景高频）
    ("累", "😴"),
    ("困", "😴"),
    ("疲惫", "😴"),
    ("无助", "🥺"),
    ("孤独", "🥺"),
    ("迷茫", "🤔"),
    ("琢磨", "🤔"),
    ("思考", "🤔"),
    ("在想", "🤔"),
    // 强正面情绪
    ("兴奋", "🤩"),
    ("雀跃", "🤩"),
    ("惊喜", "🤩"),
    ("开心", "😊"),
    ("愉悦", "😊"),
    ("快乐", "😊"),
    ("高兴", "😊"),
    ("满足", "🥰"),
    ("幸福", "🥰"),
    ("感动", "🥰"),
    ("喜欢", "🥰"),
    // 平静档（弱信号 / 中性）
    ("平静", "😌"),
    ("舒缓", "😌"),
    ("安宁", "😌"),
    ("放松", "😌"),
    // 英文备用
    ("happy", "😊"),
    ("excited", "🤩"),
    ("sad", "😢"),
    ("anxious", "😟"),
    ("tired", "😴"),
    ("calm", "😌"),
    ("frustrated", "😤"),
    ("angry", "😡"),
];

/// Pure：mood text + motion 合并查表，case-insensitive 子串首条命中返
/// emoji。**spec 反指令**：「没有 mood 数据 → 角标隐藏」对应 None；
/// **关键策略**：text + motion 都为空 → None（无数据隐藏）；非空但无匹配 →
/// 返中性 fallback "🙂"（user 已显式 record 了 mood，hide 反而违和）。
pub fn mood_to_emoji(text: &str, motion: Option<&str>) -> Option<&'static str> {
    let combined = format!(
        "{} {}",
        motion.unwrap_or("").trim(),
        text.trim()
    );
    let lower = combined.trim().to_ascii_lowercase();
    if lower.is_empty() {
        return None;
    }
    for (kw, emoji) in MOOD_EMOJI_TABLE {
        if lower.contains(&kw.to_ascii_lowercase()) {
            return Some(*emoji);
        }
    }
    // text 非空但无关键词命中 → 中性 fallback。user 已显式 record，不该完全
    // 隐藏。"🙂" 比默认 "无表情" 更轻量。
    Some("🙂")
}

/// Tauri 命令：返当前 mood 对应的 emoji（GOAL 047 figure 角标用）。
/// 文件缺失 / 空 mood → None（前端据此**隐藏**角标）；有数据 → Some(emoji)。
#[tauri::command]
pub fn get_mood_emoji() -> Option<String> {
    let (text, motion) = read_current_mood_parsed()?;
    mood_to_emoji(&text, motion.as_deref()).map(String::from)
}

/// GOAL 048：输入框 placeholder 按 pet mood 切换候选句。每个 mood "bucket"
/// 准备 3–5 条，每次启动 / mood 切换从命中 bucket 随机选一条（避免固定句固
/// 化为"标签"）。语气保持 placeholder 级（轻 / 非"开口级"，与 016
/// morning_briefing / 008 welcome_back 不重叠）。
///
/// 分桶策略（与 [`MOOD_EMOJI_TABLE`] 共用 keyword 表 spirit）：先扫强信号
/// 关键词→桶（negative / tired_thoughtful / positive / calm）；都不命中
/// fall back 到 `default` 桶（user 已 record 但词条意外）；mood 完全缺失走
/// `no_data` 桶（启动初）。
pub const PLACEHOLDER_BUCKETS: &[(&str, &[&str])] = &[
    (
        "negative",
        &[
            "怎么了？聊聊",
            "在这儿，慢慢说",
            "想聊就说，不想也行",
            "我在",
            "想说点啥都可以",
        ],
    ),
    (
        "tired_thoughtful",
        &[
            "在想什么？",
            "今天有点重，跟我说说",
            "脑子里在转啥",
            "歇会儿，聊一句",
            "想到什么就发",
        ],
    ),
    (
        "positive",
        &[
            "今天怎么样？",
            "听起来不错，说说看？",
            "有啥好玩的？",
            "分享一下吧",
            "继续呀",
        ],
    ),
    (
        "calm",
        &[
            "聊点什么？",
            "随便说说",
            "在听呢",
            "今天还顺利？",
            "想说啥",
        ],
    ),
    // user 已 record mood 但词条不在分类里——比起退回固定句更友好
    (
        "default",
        &[
            "在的，说说？",
            "聊聊？",
            "想说什么都行",
            "嗯，在听",
        ],
    ),
    // mood 文件不存在 / 解析失败——启动初最常见，比纯空 placeholder 友好
    ("no_data", &["聊点什么吧"]),
];

/// Pure：根据 mood text + motion 决定桶 key。**负面信号优先**（与
/// [`mood_to_emoji`] 同 spirit）；都不命中 → "default"。
pub fn placeholder_bucket(text: &str, motion: Option<&str>) -> &'static str {
    let combined = format!("{} {}", motion.unwrap_or("").trim(), text.trim());
    let lower = combined.trim().to_ascii_lowercase();
    if lower.is_empty() {
        return "no_data";
    }
    const NEGATIVE_KW: &[&str] = &[
        "焦虑", "担心", "不安", "烦躁", "焦躁", "生气", "愤怒", "崩溃", "沮丧",
        "低落", "难过", "伤心", "anxious", "sad", "frustrated", "angry",
    ];
    const TIRED_THOUGHTFUL_KW: &[&str] = &[
        "累", "困", "疲惫", "无助", "孤独", "迷茫", "琢磨", "思考", "在想",
        "tired",
    ];
    const POSITIVE_KW: &[&str] = &[
        "兴奋", "雀跃", "惊喜", "开心", "愉悦", "快乐", "高兴", "满足", "幸福",
        "感动", "喜欢", "happy", "excited",
    ];
    const CALM_KW: &[&str] = &["平静", "舒缓", "安宁", "放松", "calm"];
    for k in NEGATIVE_KW {
        if lower.contains(&k.to_ascii_lowercase()) {
            return "negative";
        }
    }
    for k in TIRED_THOUGHTFUL_KW {
        if lower.contains(&k.to_ascii_lowercase()) {
            return "tired_thoughtful";
        }
    }
    for k in POSITIVE_KW {
        if lower.contains(&k.to_ascii_lowercase()) {
            return "positive";
        }
    }
    for k in CALM_KW {
        if lower.contains(&k.to_ascii_lowercase()) {
            return "calm";
        }
    }
    "default"
}

/// Pure：bucket → 候选句列表。无此 bucket 返空 slice（防护性，PLACEHOLDER_
/// BUCKETS 包了 6 桶但 caller 调用 unknown key 时不应 panic）。
pub fn placeholder_candidates(bucket: &str) -> &'static [&'static str] {
    for (k, list) in PLACEHOLDER_BUCKETS {
        if *k == bucket {
            return list;
        }
    }
    &[]
}

/// Pure：在 bucket 候选里**确定性**选一条。`salt` 给 caller 传一个轻量
/// "随机因子"（ts 秒 / mood-change counter / 任意 u64），同 salt 永远返同
/// 一条——前端用启动时刻 + mood 切换计数当 salt 时，单 mood 段内 placeholder
/// 稳定（不在用户输入时跳字符）。
///
/// 不引 thread_rng——避免 placeholder 高频读时的 RNG 开销 + 让测试可预期。
pub fn pick_placeholder(bucket: &str, salt: u64) -> &'static str {
    let list = placeholder_candidates(bucket);
    if list.is_empty() {
        return "聊点什么吧"; // 终极 fallback（PLACEHOLDER_BUCKETS 不可能为空，此分支防御性）
    }
    list[(salt as usize) % list.len()]
}

/// Tauri 命令：返当前应展示的 input placeholder。`salt` 由前端传（同一段
/// mood + 同一 salt 永远返同一句，避免 re-render 时跳字符）。建议前端
/// `salt = app_session_id + mood_change_count` 一类组合。
///
/// spec「mood 数据不可用 → 退回固定句『聊点什么吧』」由 no_data bucket
/// 自然满足。
#[tauri::command]
pub fn get_input_placeholder(salt: Option<u64>) -> String {
    let s = salt.unwrap_or(0);
    let (text, motion) = match read_current_mood_parsed() {
        Some(p) => p,
        None => return pick_placeholder("no_data", s).to_string(),
    };
    let bucket = placeholder_bucket(&text, motion.as_deref());
    pick_placeholder(bucket, s).to_string()
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

    // ===== GOAL 047 mood_to_emoji tests =====
    use super::mood_to_emoji;

    #[test]
    fn mood_to_emoji_none_when_text_and_motion_empty() {
        assert_eq!(mood_to_emoji("", None), None);
        assert_eq!(mood_to_emoji("  ", Some("  ")), None);
    }

    #[test]
    fn mood_to_emoji_negative_signals_priority() {
        // 强负面信号优先于积极信号——混合表达"焦虑但还开心"取 😟
        assert_eq!(mood_to_emoji("焦虑但还开心", None), Some("😟"));
        assert_eq!(mood_to_emoji("烦躁", None), Some("😤"));
        assert_eq!(mood_to_emoji("难过", None), Some("😢"));
        assert_eq!(mood_to_emoji("崩溃了", None), Some("😭"));
    }

    #[test]
    fn mood_to_emoji_tired_thoughtful_detected() {
        assert_eq!(mood_to_emoji("有点累", None), Some("😴"));
        assert_eq!(mood_to_emoji("困得不行", None), Some("😴"));
        assert_eq!(mood_to_emoji("在想这件事", None), Some("🤔"));
        assert_eq!(mood_to_emoji("迷茫", None), Some("🤔"));
    }

    #[test]
    fn mood_to_emoji_positive_signals() {
        assert_eq!(mood_to_emoji("开心", None), Some("😊"));
        assert_eq!(mood_to_emoji("兴奋", None), Some("🤩"));
        assert_eq!(mood_to_emoji("满足", None), Some("🥰"));
        assert_eq!(mood_to_emoji("平静放松", None), Some("😌"));
    }

    #[test]
    fn mood_to_emoji_english_keywords() {
        assert_eq!(mood_to_emoji("feeling tired today", None), Some("😴"));
        assert_eq!(mood_to_emoji("ANXIOUS", None), Some("😟"));
        assert_eq!(mood_to_emoji("a bit sad", None), Some("😢"));
        assert_eq!(mood_to_emoji("calm and steady", None), Some("😌"));
    }

    #[test]
    fn mood_to_emoji_falls_back_to_neutral_when_no_match() {
        // text 非空但无关键词命中——user 已记录 mood，hide 反而违和，返中性
        assert_eq!(mood_to_emoji("今天还行", None), Some("🙂"));
        assert_eq!(mood_to_emoji("not a known mood word", None), Some("🙂"));
    }

    #[test]
    fn mood_to_emoji_combines_motion_into_scan() {
        // motion 槽里的关键词也参与扫描（虽然 Live2D motion 名通常无情绪词，
        // 但偶发 LLM 把情绪写 motion 时仍命中）
        assert_eq!(mood_to_emoji("hello", Some("Excited")), Some("🤩"));
        assert_eq!(mood_to_emoji("text without keyword", Some("Tired")), Some("😴"));
    }

    // ===== GOAL 048 placeholder bucket tests =====
    use super::{pick_placeholder, placeholder_bucket, placeholder_candidates, PLACEHOLDER_BUCKETS};

    #[test]
    fn placeholder_bucket_empty_text_and_motion_returns_no_data() {
        assert_eq!(placeholder_bucket("", None), "no_data");
        assert_eq!(placeholder_bucket("  ", Some("  ")), "no_data");
    }

    #[test]
    fn placeholder_bucket_negative_priority_over_positive() {
        // 与 mood_to_emoji 同 spirit：混合时负面信号优先
        assert_eq!(
            placeholder_bucket("焦虑但还开心", None),
            "negative"
        );
        assert_eq!(placeholder_bucket("难过", None), "negative");
    }

    #[test]
    fn placeholder_bucket_tired_thoughtful_detected() {
        assert_eq!(placeholder_bucket("有点累", None), "tired_thoughtful");
        assert_eq!(placeholder_bucket("在想", None), "tired_thoughtful");
        assert_eq!(placeholder_bucket("迷茫", None), "tired_thoughtful");
    }

    #[test]
    fn placeholder_bucket_positive_and_calm() {
        assert_eq!(placeholder_bucket("开心", None), "positive");
        assert_eq!(placeholder_bucket("兴奋", None), "positive");
        assert_eq!(placeholder_bucket("平静", None), "calm");
    }

    #[test]
    fn placeholder_bucket_unmatched_falls_back_to_default() {
        // user 已 record mood 但不在 5 关键词桶里——返 default 给友好句
        assert_eq!(placeholder_bucket("今天还行", None), "default");
        assert_eq!(placeholder_bucket("奇怪的描述", None), "default");
    }

    #[test]
    fn placeholder_candidates_all_buckets_non_empty() {
        // 所有桶必须有至少 1 句——否则 pick_placeholder 走 fallback 路径
        for (k, list) in PLACEHOLDER_BUCKETS {
            assert!(
                !list.is_empty(),
                "bucket {} 候选不能为空（spec 每桶 3-5 条）",
                k
            );
        }
    }

    #[test]
    fn placeholder_candidates_unknown_bucket_returns_empty() {
        assert!(placeholder_candidates("bogus_bucket").is_empty());
    }

    #[test]
    fn pick_placeholder_deterministic_for_same_salt() {
        // 同 salt 永远返同一条——前端用启动时刻 + mood-change counter
        // 当 salt 时，单 mood 段内 placeholder 稳定不跳字符
        let a = pick_placeholder("positive", 7);
        let b = pick_placeholder("positive", 7);
        assert_eq!(a, b);
    }

    #[test]
    fn pick_placeholder_different_salts_can_differ() {
        // 至少在桶有 ≥ 2 条时，不同 salt 应能落到不同候选
        let list = placeholder_candidates("positive");
        assert!(list.len() >= 2);
        let s0 = pick_placeholder("positive", 0);
        let s1 = pick_placeholder("positive", 1);
        assert_ne!(s0, s1, "salt 0 vs 1 在多候选桶内应落到不同句");
    }

    #[test]
    fn pick_placeholder_unknown_bucket_falls_back_to_generic() {
        let s = pick_placeholder("nonexistent", 0);
        assert_eq!(s, "聊点什么吧");
    }

    #[test]
    fn no_data_bucket_serves_fixed_fallback() {
        // spec 反指令：mood 不可用 → 固定句『聊点什么吧』
        let s = pick_placeholder("no_data", 0);
        assert_eq!(s, "聊点什么吧");
        // no_data 桶只有 1 条——salt 任意永远返同一句
        let s2 = pick_placeholder("no_data", 999);
        assert_eq!(s2, "聊点什么吧");
    }
}
