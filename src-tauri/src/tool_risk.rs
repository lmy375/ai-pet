//! Tool-call risk classifier (Iter TR2).
//!
//! Sits between the purpose gate (Iter TR1) and tool execution. Classifies every
//! call into Low / Medium / High based on tool name and (for some tools) args
//! shape, and produces a structured `ToolRiskAssessment` carrying reasons and an
//! optional safer-alternative hint.
//!
//! TR2 ships in **observe-only** mode: assessments are written to app.log so we
//! can audit what *would* be gated by TR3, but execution is not yet blocked.
//! The decision-log + log surface lets TR3 land cleanly later — flip a single
//! enforcement switch instead of also building the classifier from scratch.
//!
//! Classification policy:
//! - **High**: arbitrary shell, arbitrary file overwrite, irreversible deletes,
//!   anything that can wreck local state without a clear undo.
//! - **Medium**: scoped mutations (memory create/update, edit_file with unique
//!   old_string match), unknown / MCP tools (default until policy is set).
//! - **Low**: read-only locals, env-aware tools (active_window / weather /
//!   calendar), memory reads.

use serde::Serialize;

/// Three-tier risk band. Kept narrow on purpose — finer granularity is harder
/// to map to UI / approval flows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolRiskLevel {
    Low,
    Medium,
    High,
}

impl ToolRiskLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            ToolRiskLevel::Low => "low",
            ToolRiskLevel::Medium => "medium",
            ToolRiskLevel::High => "high",
        }
    }
}

/// Output of the classifier. Reasons are short Chinese phrases that go straight
/// into app.log / decision_log; `safe_alternative` is `Some` when there's a more
/// targeted tool the LLM could use instead (e.g. `edit_file` instead of
/// overwriting via `write_file`).
#[derive(Debug, Clone, Serialize)]
pub struct ToolRiskAssessment {
    pub risk_level: ToolRiskLevel,
    pub reasons: Vec<String>,
    pub requires_human_review: bool,
    pub safe_alternative: Option<String>,
}

/// Classify a single tool call. Pure: tool name + raw args JSON + the purpose
/// string the LLM provided (currently unused — kept as a parameter so TR2/TR3
/// follow-ups can pattern-match on purpose without changing the signature).
pub fn assess_tool_risk(tool_name: &str, args_json: &str, _purpose: &str) -> ToolRiskAssessment {
    let mut reasons: Vec<String> = Vec::new();
    let mut safe_alternative: Option<String> = None;

    let risk_level = match tool_name {
        "bash" => {
            reasons.push("可执行任意 shell 命令".to_string());
            safe_alternative = Some(
                "读文件用 read_file，修改文件用 edit_file，新建文件用 write_file，长任务后台用 check_shell_status 轮询".to_string(),
            );
            ToolRiskLevel::High
        }
        "write_file" => {
            reasons.push("会创建或完全覆盖目标文件，旧内容不可恢复".to_string());
            safe_alternative = Some("如果只是改部分内容，用 edit_file 更精确".to_string());
            ToolRiskLevel::High
        }
        "edit_file" => {
            // edit_file 要求 old_string 在文件中唯一才能改 — 这是天然的"目标性"约束。
            // 比 write_file 安全一档，但仍然写本地文件。
            reasons.push("修改本地文件内容（受 old_string 唯一性约束）".to_string());
            ToolRiskLevel::Medium
        }
        "memory_edit" => {
            let action = serde_json::from_str::<serde_json::Value>(args_json)
                .ok()
                .and_then(|v| v.get("action").and_then(|a| a.as_str()).map(String::from))
                .unwrap_or_default();
            match action.as_str() {
                "delete" => {
                    reasons.push("memory 删除不可恢复".to_string());
                    safe_alternative = Some(
                        "若只是想标记失效，可以用 update 把 description 改为已废弃说明而不是 delete".to_string(),
                    );
                    ToolRiskLevel::High
                }
                "create" | "update" => {
                    reasons.push(format!("写入宠物长期记忆（action={}）", action));
                    ToolRiskLevel::Medium
                }
                _ => {
                    // 未知 action — 走 Medium 兜底。
                    reasons.push(format!("memory_edit 未识别 action='{}'", action));
                    ToolRiskLevel::Medium
                }
            }
        }
        "read_file" => {
            reasons.push("只读访问本地文件".to_string());
            ToolRiskLevel::Low
        }
        "get_active_window"
        | "get_weather"
        | "get_upcoming_events"
        | "check_shell_status"
        | "memory_list"
        | "memory_search"
        | "memory_get" => ToolRiskLevel::Low,
        _ => {
            // 未注册的 / MCP 工具默认 Medium。一旦定义清楚单独 case。
            reasons.push(format!("未分类工具 '{}'，默认 Medium", tool_name));
            ToolRiskLevel::Medium
        }
    };

    let requires_human_review = matches!(risk_level, ToolRiskLevel::High);

    ToolRiskAssessment {
        risk_level,
        reasons,
        requires_human_review,
        safe_alternative,
    }
}

/// Render an assessment as a single compact line for app.log. Format:
/// `Tool risk [{name}]: {level}; reasons=[a, b]; review={true/false}; alt={...}`
/// Centralized so the log shape stays stable for any future log-scraper.
pub fn format_assessment_log(name: &str, a: &ToolRiskAssessment) -> String {
    let reasons = if a.reasons.is_empty() {
        "-".to_string()
    } else {
        a.reasons.join(" | ")
    };
    let alt = a
        .safe_alternative
        .as_deref()
        .map(|s| format!("; alt={}", s))
        .unwrap_or_default();
    format!(
        "Tool risk [{}]: {}; reasons=[{}]; review={}{}",
        name,
        a.risk_level.as_str(),
        reasons,
        a.requires_human_review,
        alt
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn purpose() -> &'static str {
        "stub purpose for tests"
    }

    #[test]
    fn bash_is_high_with_safe_alternative() {
        let a = assess_tool_risk("bash", "{}", purpose());
        assert_eq!(a.risk_level, ToolRiskLevel::High);
        assert!(a.requires_human_review);
        assert!(!a.reasons.is_empty(), "high risk needs an explanation");
        let alt = a.safe_alternative.expect("bash must offer a safer path");
        assert!(alt.contains("read_file") || alt.contains("edit_file"));
    }

    #[test]
    fn write_file_is_high_pointing_to_edit_file() {
        let a = assess_tool_risk(
            "write_file",
            r#"{"file_path":"/tmp/x","content":"y"}"#,
            purpose(),
        );
        assert_eq!(a.risk_level, ToolRiskLevel::High);
        assert!(a.requires_human_review);
        let alt = a
            .safe_alternative
            .expect("write_file must suggest edit_file");
        assert!(alt.contains("edit_file"));
    }

    #[test]
    fn edit_file_is_medium_no_review() {
        let a = assess_tool_risk("edit_file", "{}", purpose());
        assert_eq!(a.risk_level, ToolRiskLevel::Medium);
        assert!(!a.requires_human_review);
    }

    #[test]
    fn read_file_is_low_no_review() {
        let a = assess_tool_risk("read_file", r#"{"file_path":"/tmp/x"}"#, purpose());
        assert_eq!(a.risk_level, ToolRiskLevel::Low);
        assert!(!a.requires_human_review);
    }

    #[test]
    fn memory_edit_create_or_update_is_medium() {
        let create = assess_tool_risk("memory_edit", r#"{"action":"create"}"#, purpose());
        assert_eq!(create.risk_level, ToolRiskLevel::Medium);
        assert!(!create.requires_human_review);

        let update = assess_tool_risk("memory_edit", r#"{"action":"update"}"#, purpose());
        assert_eq!(update.risk_level, ToolRiskLevel::Medium);
    }

    #[test]
    fn memory_edit_delete_is_high_with_undo_hint() {
        let a = assess_tool_risk("memory_edit", r#"{"action":"delete"}"#, purpose());
        assert_eq!(a.risk_level, ToolRiskLevel::High);
        assert!(a.requires_human_review);
        let alt = a
            .safe_alternative
            .expect("delete must propose update-instead");
        assert!(alt.contains("update"));
    }

    #[test]
    fn memory_edit_unknown_action_falls_to_medium() {
        let a = assess_tool_risk("memory_edit", r#"{"action":"weird"}"#, purpose());
        assert_eq!(a.risk_level, ToolRiskLevel::Medium);
        assert!(a.reasons.iter().any(|r| r.contains("未识别")));
    }

    #[test]
    fn memory_edit_with_malformed_args_falls_to_medium() {
        let a = assess_tool_risk("memory_edit", "not json", purpose());
        assert_eq!(a.risk_level, ToolRiskLevel::Medium);
    }

    #[test]
    fn env_aware_tools_classified_low() {
        for name in [
            "get_active_window",
            "get_weather",
            "get_upcoming_events",
            "check_shell_status",
            "memory_list",
            "memory_search",
            "memory_get",
        ] {
            let a = assess_tool_risk(name, "{}", purpose());
            assert_eq!(a.risk_level, ToolRiskLevel::Low, "{} should be Low", name);
            assert!(!a.requires_human_review);
        }
    }

    #[test]
    fn unknown_tool_defaults_to_medium() {
        let a = assess_tool_risk("some_mcp_tool", "{}", purpose());
        assert_eq!(a.risk_level, ToolRiskLevel::Medium);
        assert!(!a.requires_human_review);
        assert!(a.reasons.iter().any(|r| r.contains("未分类")));
    }

    #[test]
    fn format_assessment_log_includes_all_fields() {
        let a = ToolRiskAssessment {
            risk_level: ToolRiskLevel::High,
            reasons: vec!["foo".to_string(), "bar".to_string()],
            requires_human_review: true,
            safe_alternative: Some("use edit_file instead".to_string()),
        };
        let s = format_assessment_log("bash", &a);
        assert!(s.contains("Tool risk [bash]"));
        assert!(s.contains("high"));
        assert!(s.contains("foo | bar"));
        assert!(s.contains("review=true"));
        assert!(s.contains("alt=use edit_file"));
    }

    #[test]
    fn format_assessment_log_handles_empty_reasons_and_no_alt() {
        // Low-risk tools often have empty reasons + no alt. Output must still be
        // well-formed (panel / log readers shouldn't see a stray ", ; ").
        let a = ToolRiskAssessment {
            risk_level: ToolRiskLevel::Low,
            reasons: vec![],
            requires_human_review: false,
            safe_alternative: None,
        };
        let s = format_assessment_log("read_file", &a);
        assert!(s.contains("Tool risk [read_file]"));
        assert!(s.contains("low"));
        assert!(s.contains("reasons=[-]"));
        assert!(s.contains("review=false"));
        assert!(!s.contains("alt="), "no-alt case must omit the alt segment");
    }
}
