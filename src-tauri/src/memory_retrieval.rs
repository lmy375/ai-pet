//! GOAL 038：跨数据源 memory retrieval。pet 能回答 "我之前提过 X 吗 /
//! 上次聊 Y 是什么时候"——不依赖 prompt window in-context 内容。
//!
//! 数据源：
//! - `memory` — PanelMemory items（全 categories）
//! - `butler_history` — butler_history.log lines（含 reminder fire /
//!   cancel / snooze / follow_up / scheduled_report 等事件）
//!
//! 检索算法：与 [[033-reminder-context-inject]] 同 tokenize 2-gram + score
//! 重叠（成熟通过 reminder_context 验证）。差不多够用；不引 embedding /
//! TF-IDF 等重型——本地、确定性、< 100ms。
//!
//! 失败语义：spec 反指令「不编造」——本模块只返检索命中；LLM 拿到结果后
//! 自行决定是否说「没找到」。

use serde::{Deserialize, Serialize};

use crate::proactive::reminder_context;

/// 检索源标识——LLM tool 的 sources 参数 enum + 输出 source 字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetrieveSource {
    Memory,
    ButlerHistory,
}

impl RetrieveSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::Memory => "memory",
            Self::ButlerHistory => "butler_history",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "memory" => Some(Self::Memory),
            "butler_history" => Some(Self::ButlerHistory),
            _ => None,
        }
    }
}

/// 全源 enum 列表——`sources=all` / 缺省时由 caller 用。
pub const ALL_SOURCES: &[RetrieveSource] = &[
    RetrieveSource::Memory,
    RetrieveSource::ButlerHistory,
];

/// 单条检索命中——返给 LLM / TG /recall。`link_id` 是源内可定位 id（memory
/// 用 title，butler_history 用 ts）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievedItem {
    pub source: String,
    /// ISO 8601 timestamp（best-effort）；某些 source 无 ts 时为空串。
    pub ts: String,
    pub text: String,
    pub link_id: String,
    pub score: usize,
}

/// Pure：从 source 列表里 parse `sources` 字符串。`"all"` / 空 → ALL。
/// 任意 token 不识别 → 跳过（不 fail，最大化兼容 LLM 拼参数小错）。
pub fn parse_sources(s: &str) -> Vec<RetrieveSource> {
    let s = s.trim();
    if s.is_empty() || s.eq_ignore_ascii_case("all") {
        return ALL_SOURCES.to_vec();
    }
    s.split(|c: char| c == ',' || c.is_whitespace())
        .filter_map(RetrieveSource::parse)
        .collect()
}

/// Pure：把一段 butler_history 文本扫成 `(ts, action, title, snippet)` 元组列表。
/// 用既有 `parse_butler_history_line`——只是把过滤 + map 收一刀。
fn parse_butler_history(content: &str) -> Vec<(String, String, String, String)> {
    content
        .lines()
        .filter(|l| !l.is_empty())
        .filter_map(crate::butler_history::parse_butler_history_line)
        .map(|(ts, action, title, snippet)| {
            (
                ts.to_string(),
                action.to_string(),
                title.to_string(),
                snippet.to_string(),
            )
        })
        .collect()
}

/// 主入口：跨 sources 检索。query 空 → 返空 vec。top_n 上限 50（防 LLM 拼
/// 巨数）。结果按 score 降序，score=0 全部 filter 掉。
pub async fn retrieve(
    query: &str,
    top_n: usize,
    sources: &[RetrieveSource],
) -> Vec<RetrievedItem> {
    let q = query.trim();
    if q.is_empty() {
        return Vec::new();
    }
    let tokens = reminder_context::tokenize_topic(q);
    if tokens.is_empty() {
        return Vec::new();
    }
    let n = top_n.clamp(1, 50);
    let mut hits: Vec<RetrievedItem> = Vec::new();

    // memory 一次性 memory_list 调用——读一次盘扫多 cat。
    let mem_index = crate::commands::memory::memory_list(None).ok();

    for src in sources {
        match src {
            RetrieveSource::Memory => {
                if let Some(ref idx) = mem_index {
                    for (_cat_name, cat) in &idx.categories {
                        for item in &cat.items {
                            let score = reminder_context::score_item(&tokens, item);
                            if score == 0 {
                                continue;
                            }
                            let snippet = reminder_context::item_snippet(item);
                            hits.push(RetrievedItem {
                                source: src.label().to_string(),
                                ts: item.created_at.clone(),
                                text: format!("「{}」 {}", item.title, snippet),
                                link_id: item.title.clone(),
                                score,
                            });
                        }
                    }
                }
            }
            RetrieveSource::ButlerHistory => {
                let history = crate::butler_history::read_history_content().await;
                for (ts, action, title, snippet) in parse_butler_history(&history) {
                    let combined = format!("{} {} {}", action, title, snippet).to_ascii_lowercase();
                    let score: usize = tokens.iter().map(|t| combined.matches(t).count()).sum();
                    if score == 0 {
                        continue;
                    }
                    hits.push(RetrievedItem {
                        source: src.label().to_string(),
                        ts: ts.clone(),
                        text: format!("[{}] {} :: {}", action, title, snippet),
                        link_id: ts,
                        score,
                    });
                }
            }
        }
    }

    hits.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| b.ts.cmp(&a.ts)));
    hits.truncate(n);
    hits
}

/// Pure：把检索结果格式化成 TG `/recall` / LLM tool 输出展示串。空命中
/// 给"没找到"诚实文案——spec 反指令「不编造」对应。
pub fn format_for_listing(query: &str, items: &[RetrievedItem]) -> String {
    if items.is_empty() {
        return format!("🔎 没找到与「{}」相关的记录。", query);
    }
    let mut lines = vec![format!("🔎 「{}」相关记录 {} 条：", query, items.len())];
    for it in items {
        let ts_short: String = it.ts.chars().take(16).collect::<String>().replace('T', " ");
        let text_short: String = it.text.chars().take(80).collect();
        lines.push(format!(
            "· [{}] {} (id: {}) {}",
            it.source, ts_short, it.link_id, text_short
        ));
    }
    lines.join("\n")
}

// ====== Tauri 命令（TG / Panel 入口） ======

#[tauri::command]
pub async fn retrieve_memory_cmd(
    query: String,
    top_n: Option<usize>,
    sources: Option<String>,
) -> Vec<RetrievedItem> {
    let srcs = parse_sources(&sources.unwrap_or_default());
    retrieve(&query, top_n.unwrap_or(5), &srcs).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_sources_all_returns_full_set() {
        assert_eq!(parse_sources("all").len(), ALL_SOURCES.len());
        assert_eq!(parse_sources("").len(), ALL_SOURCES.len());
        assert_eq!(parse_sources("ALL").len(), ALL_SOURCES.len());
    }

    #[test]
    fn parse_sources_subset_filters_to_known() {
        let s = parse_sources("memory,butler_history,bogus");
        assert_eq!(s.len(), 2);
        assert!(s.contains(&RetrieveSource::Memory));
        assert!(s.contains(&RetrieveSource::ButlerHistory));
    }

    #[test]
    fn parse_sources_whitespace_separator() {
        let s = parse_sources("memory  butler_history");
        assert_eq!(s.len(), 2);
    }

    #[test]
    fn source_label_protocol_stable() {
        // 协议字符串——LLM tool / TG / 下游 audit 依赖
        assert_eq!(RetrieveSource::Memory.label(), "memory");
        assert_eq!(RetrieveSource::ButlerHistory.label(), "butler_history");
    }

    #[test]
    fn format_for_listing_empty_query_returns_no_results_message() {
        let s = format_for_listing("买菜", &[]);
        assert!(s.contains("没找到"));
        assert!(s.contains("买菜"));
    }

    #[test]
    fn format_for_listing_includes_source_and_id() {
        let items = vec![RetrievedItem {
            source: "memory".to_string(),
            ts: "2026-05-23T10:00:00+08:00".to_string(),
            text: "「买菜」西红柿和鸡蛋".to_string(),
            link_id: "买菜".to_string(),
            score: 3,
        }];
        let s = format_for_listing("西红柿", &items);
        assert!(s.contains("[memory]"));
        assert!(s.contains("买菜"));
        assert!(s.contains("西红柿"));
    }
}
