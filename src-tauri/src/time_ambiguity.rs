//! GOAL 022：task 时间表达 ambiguity 澄清。
//!
//! 用户在创建 reminder / 011 scheduled_report / 012 deferred_task / 020
//! chain 时说出「下周末 / 傍晚 / 晚一会儿 / 过两天 / 下个月」类模糊时间
//! 词，pet 应在落库前反问而非猜测。本模块只做两件事：
//! 1. 提供「权威」的模糊词集（[`AMBIGUOUS_TIME_WORDS`]）+ 检测函数；
//! 2. 提供注入到所有 chat pipeline 的 system rule（[`inject_time_ambiguity_layer`]）
//!    教 LLM 何时反问、列几个候选、几轮 fallback。
//!
//! 没有新 LLM tool —— 澄清流由现有对话能力 + 既有 task 创建工具（butler_
//! task_edit / todo_edit / defer_task / schedule_report）走，本模块只在
//! prompt 层加约束。

use crate::commands::chat::ChatMessage;

/// 模糊时间词集合。命中即触发澄清流。
/// - 时段类：傍晚 / 早一点 / 晚一会儿 等粒度不精的副词；
/// - 相对日期：下周末 / 过两天 / 下个月 / 这两天 等无具体日期；
/// - 英文常见：later / soon / next weekend / a few days。
///
/// 不包含精确表达：「今晚 21:00 / 明天 14:00 / 5 月 4 日」这类不应触发。
pub const AMBIGUOUS_TIME_WORDS: &[&str] = &[
    // 相对日期
    "下周末",
    "这周末",
    "下个月",
    "下周",
    "下月",
    "过两天",
    "过几天",
    "这两天",
    "改天",
    "晚点",
    "晚一会儿",
    "晚一点",
    "等会儿",
    "等一下",
    // 时段
    "傍晚",
    "清晨",
    "凌晨",
    "中午前",
    "晚上",
    "早上",
    "下午",
    "上午",
    // 英文
    "later",
    "soon",
    "next weekend",
    "next month",
    "a few days",
    "in a bit",
    "evening",
    "morning",
    "afternoon",
];

/// Pure：扫文本看是否命中任一模糊词。返回去重后的命中词列表（保持
/// AMBIGUOUS_TIME_WORDS 中出现顺序）；无命中返空。case-insensitive 对
/// 英文；中文 substring 匹配。
///
/// Runtime caller：暂无 —— LLM 自己依靠 inject_time_ambiguity_layer 注入
/// 的 system rule 判断。函数留作「权威检测」给将来 Rust-side audit /
/// 验证 hook 复用（例如「LLM 没识别时强补反问」）。
#[allow(dead_code)]
pub fn find_ambiguous_words(text: &str) -> Vec<&'static str> {
    let lower = text.to_lowercase();
    let mut out: Vec<&'static str> = Vec::new();
    for w in AMBIGUOUS_TIME_WORDS {
        if lower.contains(&w.to_lowercase()) && !out.iter().any(|x| x == w) {
            out.push(*w);
        }
    }
    out
}

/// 注入层：与既有 `inject_communication_prefs_layer` 同形 —— 把「时间
/// 表达 ambiguity 澄清」rule 写成一段 system note 插在 system 段尾。
/// LLM 每轮都看到，自判触发。
///
/// 非 async（无 IO）；放在所有 chat pipeline 的 `run_chat_pipeline` 前调。
pub fn inject_time_ambiguity_layer(mut messages: Vec<ChatMessage>) -> Vec<ChatMessage> {
    let rule = format!(
        "【时间表达 ambiguity 澄清协议】用户在创建 reminder / butler_task / \
         deferred / scheduled_report 等任务时若说出模糊时间词（如「{}」等\
         {} 类常见词），**先反问澄清再落库**：\n\
         · 列出 ≤ 3 个候选具体时间，自然口吻问「是说 ... 还是 ...」；\n\
         · 用户回复任一候选 / 编号 / 自定义具体时间 → 接受 + 落库；\n\
         · 同一句仍未明确再问一次；二轮仍未明确 → 落「最早合理候选」+ \
         明告「我先按 ... 定了，要改告诉我」**不要无声丢**；\n\
         · 精确时间（「今晚 21:00」「明天 14:00」「5 月 4 日」绝对日期）\
         → 跳过反问直接落。\n\
         别为非时间的模糊表达（比如「快点写」/「认真做」）拒绝落库 —— \
         本协议只针对时间字段。",
        AMBIGUOUS_TIME_WORDS
            .iter()
            .take(8)
            .cloned()
            .collect::<Vec<_>>()
            .join("、"),
        AMBIGUOUS_TIME_WORDS.len(),
    );
    let note: ChatMessage = serde_json::from_value(serde_json::json!({
        "role": "system",
        "content": rule,
    }))
    .expect("inject_time_ambiguity_layer: static JSON shape");
    let insert_at = messages
        .iter()
        .position(|m| m.role != "system")
        .unwrap_or(messages.len());
    messages.insert(insert_at, note);
    messages
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_chinese_ambiguous_words() {
        assert!(!find_ambiguous_words("下周末提醒我看球").is_empty());
        assert!(!find_ambiguous_words("傍晚去散步").is_empty());
        assert!(!find_ambiguous_words("过两天打电话").is_empty());
    }

    #[test]
    fn detects_english_ambiguous_words() {
        let hits = find_ambiguous_words("remind me next weekend");
        assert!(hits.contains(&"next weekend"));
    }

    #[test]
    fn precise_time_does_not_trigger() {
        // 「今晚 21:00」「明天 14:00」「2026-05-04 09:00」精确表达不该命中
        assert!(find_ambiguous_words("今晚 21:00 提醒我").is_empty());
        assert!(find_ambiguous_words("2026-05-04 09:00 开会").is_empty());
    }

    #[test]
    fn deduplicates_repeated_hits() {
        // 同一词出现两次只算一次
        let hits = find_ambiguous_words("下周末又下周末");
        assert_eq!(hits.iter().filter(|w| **w == "下周末").count(), 1);
    }

    #[test]
    fn unrelated_text_returns_empty() {
        assert!(find_ambiguous_words("写完 Q3 报告然后发给 boss").is_empty());
        assert!(find_ambiguous_words("").is_empty());
    }

    #[test]
    fn inject_layer_inserts_after_system_messages() {
        let msgs = vec![
            ChatMessage {
                role: "system".to_string(),
                content: serde_json::Value::String("soul".to_string()),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            },
            ChatMessage {
                role: "user".to_string(),
                content: serde_json::Value::String("hi".to_string()),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            },
        ];
        let out = inject_time_ambiguity_layer(msgs);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].role, "system"); // soul
        assert_eq!(out[1].role, "system"); // injected rule
        assert_eq!(out[2].role, "user"); // pushed back
        let body = out[1].content.as_str().unwrap_or("");
        assert!(body.contains("ambiguity"));
        assert!(body.contains("反问"));
    }

    #[test]
    fn evening_morning_afternoon_are_ambiguous() {
        // 通用时段词：覆盖
        assert!(!find_ambiguous_words("evening run").is_empty());
        assert!(!find_ambiguous_words("morning meeting").is_empty());
    }
}
