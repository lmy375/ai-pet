//! 工具审核策略：在 `tool_risk::assess_tool_risk` 自动分级的输出之上叠
//! 一层"用户偏好"，让用户在面板里逐个工具选 **总是放行 / 总是审核 /
//! 自动**。
//!
//! 与 `tool_risk` 的关系：
//! - 自动分级是真相源 — 给出"这个调用客观风险多大"的判断
//! - 本模块是用户偏好层 — 决定"此次调用要不要进 ToolReviewRegistry 等用户点同意"
//!
//! 全部纯函数 + 静态 metadata。任何 IO（读 settings、查 registry）由
//! `commands/chat.rs` 在调用点完成。

use serde::Serialize;

use crate::commands::settings::get_settings;
use crate::tools::BUILTIN_TOOL_NAMES;

/// 用户对单个工具的审核偏好。
///
/// - `Auto`：跟着分类器的 `requires_human_review` 走（默认）。
/// - `AlwaysReview`：哪怕分类器说"低风险放行"也强制走 panel 审核。给"洁
///   癖型"用户在用 LLM 跑陌生工具时上一道保险。
/// - `AlwaysApprove`：哪怕分类器说"高风险须审核"也直接放行。给"自动化
///   程度高、宁可吃风险"的用户在批量运行场景里关掉打扰。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolReviewMode {
    Auto,
    AlwaysReview,
    AlwaysApprove,
}

impl ToolReviewMode {
    pub fn as_str(self) -> &'static str {
        match self {
            ToolReviewMode::Auto => "auto",
            ToolReviewMode::AlwaysReview => "always_review",
            ToolReviewMode::AlwaysApprove => "always_approve",
        }
    }
}

/// 把 yaml / 前端传入的字符串解析成 `ToolReviewMode`。**未知值退回 Auto** —
/// 让前向兼容自然成立：日后加新 mode 时旧前端 / 旧配置不会因为不识别
/// 而崩，也不会被误升级到一个想不到的状态。
pub fn parse_mode(s: &str) -> ToolReviewMode {
    match s.trim() {
        "always_review" => ToolReviewMode::AlwaysReview,
        "always_approve" => ToolReviewMode::AlwaysApprove,
        // "auto" 与 一切未识别值都视作 Auto
        _ => ToolReviewMode::Auto,
    }
}

/// 给定分类器的"是否需要审核"判定 + 用户偏好，返回**最终**是否进 registry。
/// 三态语义：
/// - AlwaysApprove：直接 false（哪怕高危也跳过审核）
/// - AlwaysReview：直接 true（哪怕低危也强制审核）
/// - Auto：透传 `auto_required`
pub fn effective_requires_review(auto_required: bool, mode: ToolReviewMode) -> bool {
    match mode {
        ToolReviewMode::AlwaysApprove => false,
        ToolReviewMode::AlwaysReview => true,
        ToolReviewMode::Auto => auto_required,
    }
}

/// 给面板用的"基线风险"标签。和 `assess_tool_risk` 的逐次输出区分：
/// 这里回答"该工具最严重时的风险等级 + 一句话原因"，让用户能一眼判
/// 断要不要覆盖。不依赖 args（args 形态因调用而异，但用户做静态偏好
/// 设置时关注的是工具本身的最大破坏力）。
#[derive(Debug, Clone, Serialize)]
pub struct NominalRisk {
    /// "high" / "medium" / "low" — 与 ToolRiskLevel 的 `as_str` 同形，
    /// 让前端徽章色卡逻辑可以共用。
    pub level: &'static str,
    /// 一句话原因，最长 ~30 字符的中文短句。让用户不必读代码也能知道
    /// "为什么这工具被标 high"。
    pub note: &'static str,
}

/// 内置工具的基线风险表。为什么硬编码而不是调 `assess_tool_risk("name", "{}", "")`：
/// - assess 在 args 不完整时会给出 misleading 的 fallback（如 memory_edit 空
///   args 返回 Medium，但其实最严重时 delete 是 High）—— 用户做偏好设置
///   时需要看"最严重情况"
/// - 静态表更稳定 —— 改动 assess 内部分级时不必担心 UI 标签跟着错位
///
/// 未在表里的工具（含 MCP）→ 默认 Medium + 「未分类工具」。
pub fn nominal_risk_label(tool_name: &str) -> NominalRisk {
    match tool_name {
        "bash" => NominalRisk {
            level: "high",
            note: "可执行任意 shell 命令",
        },
        "write_file" => NominalRisk {
            level: "high",
            note: "覆写文件，旧内容不可恢复",
        },
        "edit_file" => NominalRisk {
            level: "medium",
            note: "改本地文件（受 old_string 唯一性约束）",
        },
        "memory_edit" => NominalRisk {
            level: "high",
            note: "delete 操作不可恢复；create/update 中等",
        },
        "read_file" => NominalRisk {
            level: "low",
            note: "只读访问本地文件",
        },
        "memory_list" | "memory_search" | "memory_get" => NominalRisk {
            level: "low",
            note: "只读记忆查询",
        },
        "get_active_window" | "get_weather" | "get_upcoming_events" | "check_shell_status" => {
            NominalRisk {
                level: "low",
                note: "只读环境感知工具",
            }
        }
        "propose_task" => NominalRisk {
            level: "low",
            note: "仅向用户提议任务，无副作用",
        },
        "task_create" => NominalRisk {
            level: "medium",
            note: "直接写入任务队列（无确认卡，仅 TG 等无 UI 入口使用）",
        },
        _ => NominalRisk {
            level: "medium",
            note: "未分类工具，默认 medium",
        },
    }
}

/// 一行工具的 panel 视图：名 / 风险 / 备注 / 当前用户偏好。
#[derive(Debug, Clone, Serialize)]
pub struct ToolRiskOverviewRow {
    pub name: String,
    pub level: &'static str,
    pub note: &'static str,
    /// "auto" / "always_review" / "always_approve"
    pub mode: &'static str,
}

/// Tauri 命令：返回内置工具的"风险 + 当前用户偏好"清单。读 settings
/// 失败时仍返回完整列表，所有 mode 退回 "auto"，保证面板永远能渲染。
#[tauri::command]
pub fn get_tool_risk_overview() -> Vec<ToolRiskOverviewRow> {
    let overrides = get_settings()
        .ok()
        .map(|s| s.tool_review_overrides)
        .unwrap_or_default();
    BUILTIN_TOOL_NAMES
        .iter()
        .map(|&name| {
            let nr = nominal_risk_label(name);
            let mode = overrides
                .get(name)
                .map(|v| parse_mode(v))
                .unwrap_or(ToolReviewMode::Auto)
                .as_str();
            ToolRiskOverviewRow {
                name: name.to_string(),
                level: nr.level,
                note: nr.note,
                mode,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ----- parse_mode -----

    #[test]
    fn parse_known_modes() {
        assert_eq!(parse_mode("auto"), ToolReviewMode::Auto);
        assert_eq!(parse_mode("always_review"), ToolReviewMode::AlwaysReview);
        assert_eq!(parse_mode("always_approve"), ToolReviewMode::AlwaysApprove);
    }

    #[test]
    fn parse_trims_whitespace() {
        assert_eq!(parse_mode("  always_review  "), ToolReviewMode::AlwaysReview);
    }

    #[test]
    fn parse_unknown_falls_back_to_auto() {
        // 前向兼容：旧前端见到新 mode → 不认识 → 退回 Auto
        assert_eq!(parse_mode("future_mode_xyz"), ToolReviewMode::Auto);
        assert_eq!(parse_mode(""), ToolReviewMode::Auto);
        assert_eq!(parse_mode("ALWAYS_REVIEW"), ToolReviewMode::Auto); // 大小写敏感
    }

    // ----- effective_requires_review -----

    #[test]
    fn auto_passes_through_classifier_decision() {
        assert!(effective_requires_review(true, ToolReviewMode::Auto));
        assert!(!effective_requires_review(false, ToolReviewMode::Auto));
    }

    #[test]
    fn always_approve_overrides_high_risk() {
        // 用户说"放行"就放行，哪怕分类器要求审核
        assert!(!effective_requires_review(true, ToolReviewMode::AlwaysApprove));
        assert!(!effective_requires_review(false, ToolReviewMode::AlwaysApprove));
    }

    #[test]
    fn always_review_overrides_low_risk() {
        // 用户说"审核"就审核，哪怕分类器说低风险
        assert!(effective_requires_review(true, ToolReviewMode::AlwaysReview));
        assert!(effective_requires_review(false, ToolReviewMode::AlwaysReview));
    }

    // ----- nominal_risk_label -----

    #[test]
    fn nominal_label_marks_dangerous_tools_high() {
        assert_eq!(nominal_risk_label("bash").level, "high");
        assert_eq!(nominal_risk_label("write_file").level, "high");
        // memory_edit 整体标 high — 用户在 UI 上看到的是"最严重情况"
        assert_eq!(nominal_risk_label("memory_edit").level, "high");
    }

    #[test]
    fn nominal_label_marks_readonly_tools_low() {
        assert_eq!(nominal_risk_label("read_file").level, "low");
        assert_eq!(nominal_risk_label("memory_list").level, "low");
        assert_eq!(nominal_risk_label("memory_search").level, "low");
        assert_eq!(nominal_risk_label("get_weather").level, "low");
        assert_eq!(nominal_risk_label("get_active_window").level, "low");
        assert_eq!(nominal_risk_label("propose_task").level, "low");
    }

    #[test]
    fn nominal_label_marks_constrained_writes_medium() {
        // edit_file 受 old_string 唯一性约束 — 比 write_file 安全一档
        assert_eq!(nominal_risk_label("edit_file").level, "medium");
    }

    #[test]
    fn nominal_label_unknown_tool_defaults_medium() {
        // MCP 工具 / 未注册名字 — 默认 medium，与分类器 fallback 对齐
        let nr = nominal_risk_label("some_mcp_tool_xyz");
        assert_eq!(nr.level, "medium");
        assert!(nr.note.contains("未分类"));
    }

    #[test]
    fn nominal_label_provides_human_readable_note() {
        // 每个内置工具都应有非空 note —— UI 直接展示给用户
        for name in [
            "bash",
            "write_file",
            "edit_file",
            "memory_edit",
            "read_file",
            "memory_list",
            "memory_search",
            "get_active_window",
            "get_weather",
            "get_upcoming_events",
            "check_shell_status",
            "propose_task",
            "task_create",
        ] {
            let nr = nominal_risk_label(name);
            assert!(!nr.note.is_empty(), "tool {} has empty note", name);
        }
    }
}
