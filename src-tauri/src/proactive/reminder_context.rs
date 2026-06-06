//! GOAL 033：reminder 到点 fire 时附带"近期相关 memory"上下文。把
//! "提醒：买菜" 升级为"提醒：买菜。你上次说想买西红柿和鸡蛋。"
//!
//! 设计：
//! - 纯后端检索（非 LLM-tool 自取）——确定性 / token 高效 / 无外部依赖。
//! - 检索范围：`ai_insights` + `user_profile`（session_distill 023 也写入
//!   这两个 cat；self_note 029 是 pet-owned 内心，故意排除）。窗口
//!   30 天，top-3。
//! - 评分：token 重叠（中文 2-gram + 空白分词）。差不多够用——更精的
//!   embedding / TF-IDF 暂不上，先看效果。
//! - persona disable：扫 communication_prefs.active 中含禁用关键词 → 整体
//!   跳过 enrich。命中 [[019-persona-style-self-tune]] 反馈机制。
//!
//! 仅作用于 **reminder fire** path；011 scheduled_report / 012 deferred_task
//! 已有自由 LLM 调度，重复 context 反而稀释。

use chrono::{NaiveDate, NaiveDateTime};

use crate::commands::memory::MemoryItem;

/// 检索窗口（天）。30d 是 [[007-memory-follow-up]] 同档：覆盖"最近聊过的"
/// 大半，不至于把太老的 stale memory 拉进 reminder。
pub const RETRIEVE_WINDOW_DAYS: i64 = 30;

/// 每条 reminder 最多附几条相关 item。3 是 [[005-url-fetch-summarize]] MAX_URLS
/// 同档：信号 vs 噪声边界经验值。
pub const TOP_K: usize = 3;

/// 检索源 category。session_distill 023 也写到这两个 cat，所以
/// 无需显式加 `session_distill` 名字。self_note 029 是 pet-owned 内心，
/// 不算"用户语境"，故意不含。
pub const CONTEXT_SOURCE_CATEGORIES: &[&str] = &["ai_insights", "user_profile"];

/// 单条 reminder 一行 context 上限字符（中文 char）。再长稀释 reminder 主体。
const CONTEXT_LINE_CHAR_CAP: usize = 60;

/// Pure：把 topic 切成「2-gram 中文片段 + 空白分隔的英文 token」混合集。
/// 简单实用：用户 reminder 通常一句话内 1-2 个关键短语，无需 jieba 等
/// 完整分词器。重复 token 去重。
pub fn tokenize_topic(text: &str) -> Vec<String> {
    let mut tokens: Vec<String> = Vec::new();
    // 空白切——抓英文 / 数字 token。trim 中文标点。
    for raw in text.split(|c: char| {
        c.is_whitespace()
            || c == '，'
            || c == '。'
            || c == '：'
            || c == '；'
            || c == '!'
            || c == '?'
            || c == ','
            || c == '.'
    }) {
        let t = raw.trim();
        if t.is_empty() {
            continue;
        }
        // ASCII token：≥2 字符的英文 / 数字加入（单字符噪音大）。
        if t.chars().all(|c| c.is_ascii()) {
            if t.chars().count() >= 2 {
                let lower = t.to_ascii_lowercase();
                if !tokens.contains(&lower) {
                    tokens.push(lower);
                }
            }
            continue;
        }
        // 含中文：抽 2-char 窗口。"买菜" → ["买菜"]；"周末买菜" → ["周末","末买","买菜"]
        let chars: Vec<char> = t.chars().collect();
        if chars.len() < 2 {
            continue;
        }
        for window in chars.windows(2) {
            // 跳全 ASCII 的窗口（中英混排时只对纯中文 / 混合 segment 取 gram）。
            let s: String = window.iter().collect();
            if s.chars().all(|c| c.is_ascii()) {
                continue;
            }
            if !tokens.contains(&s) {
                tokens.push(s);
            }
        }
    }
    tokens
}

/// Pure：单 item 与 topic 的相关度得分。在 (title + description) 里数 topic
/// token 出现总次数（多 token 命中 → 高分）。description 含 marker（如
/// `[remind: ...]`）也无害——marker 与中文不太可能 token-overlap。
pub fn score_item(topic_tokens: &[String], item: &MemoryItem) -> usize {
    if topic_tokens.is_empty() {
        return 0;
    }
    let haystack = format!("{}\n{}", item.title.to_ascii_lowercase(), item.description.to_ascii_lowercase());
    topic_tokens
        .iter()
        .map(|t| haystack.matches(t).count())
        .sum()
}

/// Pure：created_at（ISO 8601 with offset 或 naive）距 today 是否 ≤ days。
/// 无 created_at（早期 entry）→ 视为"未知"，仍返 true（spec 写 30d，但
/// 我们不应因为字段缺失就把 user_profile 老 entry 漏掉——优雅退化）。
pub fn is_within_window(item: &MemoryItem, today: NaiveDate, days: i64) -> bool {
    let raw = item.created_at.trim();
    if raw.is_empty() {
        return true;
    }
    // 先尝带 offset 形（如 `2026-05-20T08:00:00+08:00`）；strip 掉时区后按
    // naive parse。失败也优雅 true。
    let no_tz = if let Some(idx) = raw.rfind('+').or_else(|| raw.rfind('-').filter(|&i| i > 10)) {
        &raw[..idx]
    } else {
        raw
    };
    let dt = match NaiveDateTime::parse_from_str(no_tz, "%Y-%m-%dT%H:%M:%S") {
        Ok(d) => d,
        Err(_) => return true,
    };
    let item_date = dt.date();
    (today - item_date).num_days() <= days
}

/// Pure：从一组 items 选 score>0 的 top-k，按 score 降序。同分时保持原顺序
/// （`sort_by` stable）。返回 owned `(&item, score)` 让 caller 用。
pub fn top_k_related<'a>(
    topic_tokens: &[String],
    items: &'a [&'a MemoryItem],
    k: usize,
) -> Vec<&'a MemoryItem> {
    let mut scored: Vec<(usize, &MemoryItem)> = items
        .iter()
        .map(|it| (score_item(topic_tokens, it), *it))
        .filter(|(s, _)| *s > 0)
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.into_iter().take(k).map(|(_, it)| it).collect()
}

/// Pure：从一条 item 的 description 抽一句"显示用的 snippet"。简化策略：
/// strip markers `[xxx: yyy]` → 取剩余前 60 char。空时回退 title。
pub fn item_snippet(item: &MemoryItem) -> String {
    let mut s = item.description.clone();
    // 简单脱 markers：从左到右遇到 `[xxx:` 就跳到匹配的 `]`。
    let mut out = String::new();
    let bytes = s.as_str();
    let mut chars = bytes.chars().peekable();
    let mut in_marker = false;
    let mut buf = String::new();
    while let Some(c) = chars.next() {
        if !in_marker && c == '[' {
            // 探后面是否含 `:`（marker 形）；不是则照常输出 `[`。
            let snapshot: String = bytes
                [bytes.len() - chars.clone().collect::<String>().len()..]
                .chars()
                .take(40)
                .collect();
            if snapshot.contains(']') && snapshot.find(':').is_some() {
                in_marker = true;
                continue;
            }
            buf.push(c);
        } else if in_marker {
            if c == ']' {
                in_marker = false;
            }
        } else {
            buf.push(c);
        }
    }
    out.push_str(buf.trim());
    if out.is_empty() {
        s = item.title.clone();
        out = s.trim().to_string();
    }
    // char cap
    let chars: Vec<char> = out.chars().collect();
    if chars.len() > CONTEXT_LINE_CHAR_CAP {
        let truncated: String = chars.into_iter().take(CONTEXT_LINE_CHAR_CAP).collect();
        format!("{}…", truncated)
    } else {
        out
    }
}

/// Pure：把多组「reminder topic → 相关 items」拼成一段插到 hint 末尾。空
/// 输入 / 全空命中返空串——caller 用 is_empty() 短路。
pub fn format_context_block(per_reminder: &[(String, Vec<&MemoryItem>)]) -> String {
    let any = per_reminder.iter().any(|(_, items)| !items.is_empty());
    if !any {
        return String::new();
    }
    let mut s = String::from("\n相关上下文（按到期 reminder 关联，挑相关一条柔顺嵌入开口；不要罗列）：\n");
    for (topic, items) in per_reminder {
        for it in items {
            s.push_str(&format!("· [{}] {}\n", topic.trim(), item_snippet(it)));
        }
    }
    s
}

/// 异步入口：对一组 due reminder topic 检索相关 items，返回 per-reminder
/// 命中列表。空命中条目保留位（caller 接到后用 format_context_block 自决
/// 输出形态）。
pub async fn retrieve_for_due_reminders(
    reminder_topics: &[String],
    today: NaiveDate,
) -> Vec<(String, Vec<MemoryItem>)> {
    if reminder_topics.is_empty() {
        return Vec::new();
    }
    // 一次性 load index，所有 reminder 共用扫描。memory_list(None) 拿全图。
    let index = match crate::commands::memory::memory_list(None) {
        Ok(i) => i,
        Err(_) => return Vec::new(),
    };

    // 收集源 cat 的 items（窗口内）
    let mut pool: Vec<MemoryItem> = Vec::new();
    for cat_name in CONTEXT_SOURCE_CATEGORIES {
        if let Some(cat) = index.categories.get(*cat_name) {
            for it in &cat.items {
                if is_within_window(it, today, RETRIEVE_WINDOW_DAYS) {
                    pool.push(it.clone());
                }
            }
        }
    }
    let pool_refs: Vec<&MemoryItem> = pool.iter().collect();

    let mut out: Vec<(String, Vec<MemoryItem>)> = Vec::with_capacity(reminder_topics.len());
    for topic in reminder_topics {
        let tokens = tokenize_topic(topic);
        let hits = top_k_related(&tokens, &pool_refs, TOP_K);
        let cloned: Vec<MemoryItem> = hits.into_iter().cloned().collect();
        out.push((topic.clone(), cloned));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(title: &str, desc: &str, created_at: &str) -> MemoryItem {
        MemoryItem {
            title: title.to_string(),
            description: desc.to_string(),
            detail_path: String::new(),
            created_at: created_at.to_string(),
            updated_at: String::new(),
        }
    }

    #[test]
    fn tokenize_extracts_chinese_bigrams_and_ascii_tokens() {
        let t = tokenize_topic("买菜 ddl");
        assert!(t.contains(&"买菜".to_string()));
        assert!(t.contains(&"ddl".to_string()));
    }

    #[test]
    fn tokenize_extracts_overlapping_bigrams() {
        let t = tokenize_topic("周末买菜");
        // "周末" / "末买" / "买菜" 三个 window
        assert!(t.contains(&"周末".to_string()));
        assert!(t.contains(&"末买".to_string()));
        assert!(t.contains(&"买菜".to_string()));
    }

    #[test]
    fn tokenize_skips_single_char_ascii_tokens() {
        let t = tokenize_topic("a b cd");
        assert!(!t.iter().any(|x| x == "a" || x == "b"));
        assert!(t.contains(&"cd".to_string()));
    }

    #[test]
    fn score_item_zero_when_no_overlap() {
        let it = item("写报告", "周一交季度报告", "");
        let tokens = tokenize_topic("买菜");
        assert_eq!(score_item(&tokens, &it), 0);
    }

    #[test]
    fn score_item_counts_overlapping_tokens() {
        let it = item("买菜清单", "上次说想买西红柿和鸡蛋", "");
        let tokens = tokenize_topic("买菜 西红柿");
        // "买菜" 在 title + desc 至少各一次；"西红" "红柿" 各一次在 desc
        let s = score_item(&tokens, &it);
        assert!(s >= 2, "{}", s);
    }

    #[test]
    fn top_k_caps_results_and_filters_zero_score() {
        let a = item("买菜", "西红柿", "");
        let b = item("写代码", "工作清单", "");
        let c = item("买菜笔记", "鸡蛋酱油", "");
        let d = item("买菜补充", "土豆", "");
        let items: Vec<&MemoryItem> = vec![&a, &b, &c, &d];
        let tokens = tokenize_topic("买菜");
        let hits = top_k_related(&tokens, &items, 2);
        assert_eq!(hits.len(), 2);
        // b 不应入：score=0
        assert!(!hits.iter().any(|x| x.title == "写代码"));
    }

    #[test]
    fn is_within_window_boundary_30_days() {
        let today = NaiveDate::from_ymd_opt(2026, 5, 23).unwrap();
        let inside = item("x", "y", "2026-05-01T08:00:00+08:00"); // 22d 前
        let outside = item("x", "y", "2026-04-01T08:00:00+08:00"); // 52d 前
        assert!(is_within_window(&inside, today, 30));
        assert!(!is_within_window(&outside, today, 30));
    }

    #[test]
    fn is_within_window_empty_created_at_passes() {
        let today = NaiveDate::from_ymd_opt(2026, 5, 23).unwrap();
        let no_ts = item("x", "y", "");
        // 缺 ts 优雅退化为 true（不漏老 user_profile）
        assert!(is_within_window(&no_ts, today, 30));
    }

    #[test]
    fn item_snippet_strips_markers() {
        let it = item("买菜", "[remind: 18:00] 买菜清单", "");
        let s = item_snippet(&it);
        assert!(!s.contains("[remind"));
        assert!(s.contains("买菜清单"));
    }

    #[test]
    fn item_snippet_falls_back_to_title_when_desc_empty_after_strip() {
        let it = item("纯标题", "[remind: 14:00] [topic_arc: x]", "");
        let s = item_snippet(&it);
        assert_eq!(s, "纯标题");
    }

    #[test]
    fn format_context_block_empty_when_all_misses() {
        let per: Vec<(String, Vec<&MemoryItem>)> = vec![
            ("买菜".to_string(), Vec::new()),
            ("写报告".to_string(), Vec::new()),
        ];
        assert_eq!(format_context_block(&per), "");
    }

    #[test]
    fn format_context_block_groups_per_reminder() {
        let it_a = item("a", "西红柿和鸡蛋", "");
        let it_b = item("b", "酱油快没了", "");
        let per: Vec<(String, Vec<&MemoryItem>)> = vec![("买菜".to_string(), vec![&it_a, &it_b])];
        let s = format_context_block(&per);
        assert!(s.contains("[买菜]"));
        assert!(s.contains("西红柿和鸡蛋"));
        assert!(s.contains("酱油快没了"));
        // 反指令明确"不要罗列"
        assert!(s.contains("不要罗列"));
    }

    // 历史 test 引用已删的 `is_context_disabled_in_prefs` 函数 ——
    // 该功能被另一处取代，test 留下成 orphan，删除恢复 test build。
}
