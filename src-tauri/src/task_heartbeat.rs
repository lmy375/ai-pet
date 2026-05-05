//! 长任务心跳：检测"被宠物动过手却停滞超过阈值的 pending 任务"，把
//! 这条信号编进下次 proactive prompt，让 LLM 要么写一句进展、要么标
//! [done] / [error: ...]，避免任务在 butler_tasks 队列里悄悄烂掉。
//!
//! 本模块只装**纯函数**：心跳判定 + 提示文本格式。任何 IO（读
//! butler_tasks、读 settings、过 redaction）由 `proactive.rs` 在外层
//! 调用 `build_task_heartbeat_hint` 时处理。这条边界与
//! `task_queue.rs` 同源 —— 让所有"是不是该提醒"决策可单测。
//!
//! 推断"任务在飞"的依据是 `updated_at > created_at + TOUCHED_EPSILON_SECS`，
//! 不要求 LLM 写新协议（如 `[running]` 标记）—— 只要它走 `memory_edit`
//! 写过一次进度，updated_at 就会前进。这种无侵入推断让心跳能与已有所有
//! 任务执行路径正交。

use chrono::NaiveDateTime;

use crate::proactive::parse_updated_at_local;
use crate::task_queue::{classify_status, TaskStatus};

/// `updated_at - created_at` 至少要超过这个秒数才算"LLM 已触碰"。
/// 防御性：memory_edit 在创建时也会更新 `updated_at`（即便和 created_at
/// 同一时刻），但理论上两个时间戳应当一致。给个 5s 的窗口 cover 时钟
/// 抖动，避免刚建好的任务被立刻当成"在飞"。
pub const TOUCHED_EPSILON_SECS: i64 = 5;

/// 心跳判定。返回 `true` 仅当 task 满足全部 4 个条件：
/// 1. 状态为 Pending；
/// 2. created_at / updated_at 都能解析为本地时间；
/// 3. updated_at - created_at >= TOUCHED_EPSILON_SECS（LLM 真的 update 过）；
/// 4. now - updated_at >= threshold_minutes 分钟。
///
/// `threshold_minutes == 0` 视作禁用，永远返回 false。
pub fn is_heartbeat_candidate(
    description: &str,
    created_at: &str,
    updated_at: &str,
    now: NaiveDateTime,
    threshold_minutes: u32,
) -> bool {
    if threshold_minutes == 0 {
        return false;
    }
    if classify_status(description).0 != TaskStatus::Pending {
        return false;
    }
    let Some(created): Option<NaiveDateTime> = parse_updated_at_local(created_at) else {
        return false;
    };
    let Some(updated): Option<NaiveDateTime> = parse_updated_at_local(updated_at) else {
        return false;
    };
    let touched_secs = updated.signed_duration_since(created).num_seconds();
    if touched_secs < TOUCHED_EPSILON_SECS {
        return false;
    }
    let stalled_secs = now.signed_duration_since(updated).num_seconds();
    stalled_secs >= (threshold_minutes as i64) * 60
}

/// 把命中的标题列表渲染成 prompt 段落。
///
/// - 标题列表为空 / `threshold_minutes == 0` → 返回空字符串（`push_if_nonempty`
///   会自动跳过）。
/// - 1 条 → 单数措辞「你正在做的「X」已经超过 N 分钟没动了…」。
/// - ≥ 2 条 → 列表 + 复数措辞，但 instruction 段保持一致，让 LLM 知道
///   每一条都需要在这一轮里给出更新或闭合。
pub fn format_heartbeat_hint(titles: &[String], threshold_minutes: u32) -> String {
    if titles.is_empty() || threshold_minutes == 0 {
        return String::new();
    }
    let instruction = format!(
        "请这一轮要么写一句进展（用 `memory_edit update` 在描述里加状态），\
要么标记 `[done]` / `[error: 原因]`，别让用户委托给你的事在队列里悄悄烂掉。"
    );
    if titles.len() == 1 {
        format!(
            "[心跳] 你正在做的「{}」已经超过 {} 分钟没动了。{}",
            titles[0].trim(),
            threshold_minutes,
            instruction
        )
    } else {
        let bullets: Vec<String> = titles
            .iter()
            .map(|t| format!("- {}", t.trim()))
            .collect();
        format!(
            "[心跳] 这些任务已经超过 {} 分钟没动了：\n{}\n{}",
            threshold_minutes,
            bullets.join("\n"),
            instruction
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, NaiveDate};

    fn naive(y: i32, m: u32, d: u32, hh: u32, mm: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(hh, mm, 0)
            .unwrap()
    }

    /// 构造与 memory_edit 写入格式相同的 ISO 字符串。+08:00 是 China/Local
    /// 的常见 fixture；解析路径只关心能 round-trip 到 NaiveDateTime。
    fn iso(y: i32, m: u32, d: u32, hh: u32, mm: u32, ss: u32) -> String {
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}+08:00",
            y, m, d, hh, mm, ss
        )
    }

    fn now_after(updated: NaiveDateTime, minutes: i64) -> NaiveDateTime {
        updated + Duration::minutes(minutes)
    }

    // ------------- is_heartbeat_candidate -------------

    #[test]
    fn skips_when_threshold_zero() {
        // threshold=0 当作禁用：哪怕其它条件满足也不触发
        let updated = naive(2026, 5, 4, 12, 0);
        assert!(!is_heartbeat_candidate(
            "[task pri=1] x",
            &iso(2026, 5, 4, 11, 0, 0),
            &iso(2026, 5, 4, 11, 30, 0),
            now_after(updated, 999),
            0,
        ));
    }

    #[test]
    fn skips_when_status_done() {
        let created = iso(2026, 5, 4, 11, 0, 0);
        let updated = iso(2026, 5, 4, 11, 30, 0);
        assert!(!is_heartbeat_candidate(
            "[task pri=1] x [done]",
            &created,
            &updated,
            naive(2026, 5, 4, 13, 0),
            30,
        ));
    }

    #[test]
    fn skips_when_status_error() {
        // error 状态用户已经能在 panel 看到"失败"徽章，心跳重复打扰没意义
        assert!(!is_heartbeat_candidate(
            "[task pri=1] [error: 文件不存在] x",
            &iso(2026, 5, 4, 11, 0, 0),
            &iso(2026, 5, 4, 11, 30, 0),
            naive(2026, 5, 4, 13, 0),
            30,
        ));
    }

    #[test]
    fn skips_when_never_touched() {
        // updated_at 与 created_at 相同 → LLM 还从未 update 过；
        // 这种"积压未启动"不在心跳语义里
        let same = iso(2026, 5, 4, 11, 0, 0);
        assert!(!is_heartbeat_candidate(
            "[task pri=1] x",
            &same,
            &same,
            naive(2026, 5, 4, 14, 0),
            30,
        ));
    }

    #[test]
    fn skips_when_touch_gap_below_epsilon() {
        // 间隔 < TOUCHED_EPSILON_SECS（5s）认为是 memory_edit 自身的时间戳
        // 抖动，不算"真的触碰过"
        assert!(!is_heartbeat_candidate(
            "[task pri=1] x",
            &iso(2026, 5, 4, 11, 0, 0),
            &iso(2026, 5, 4, 11, 0, 3),
            naive(2026, 5, 4, 14, 0),
            30,
        ));
    }

    #[test]
    fn fires_at_threshold_boundary() {
        // 整整 30 分钟未动：>= 阈值算命中
        let created = iso(2026, 5, 4, 11, 0, 0);
        let updated_naive = naive(2026, 5, 4, 11, 30);
        let updated_iso = iso(2026, 5, 4, 11, 30, 0);
        let now = updated_naive + Duration::minutes(30);
        assert!(is_heartbeat_candidate(
            "[task pri=1] 整理 Downloads",
            &created,
            &updated_iso,
            now,
            30,
        ));
    }

    #[test]
    fn skips_when_within_threshold() {
        let created = iso(2026, 5, 4, 11, 0, 0);
        let updated_naive = naive(2026, 5, 4, 11, 30);
        let updated_iso = iso(2026, 5, 4, 11, 30, 0);
        let now = updated_naive + Duration::minutes(29);
        assert!(!is_heartbeat_candidate(
            "[task pri=1] 整理 Downloads",
            &created,
            &updated_iso,
            now,
            30,
        ));
    }

    #[test]
    fn skips_when_timestamps_unparseable() {
        // 历史脏数据 / 手改 yaml 不应导致心跳误判 — parse 失败 = 跳过
        assert!(!is_heartbeat_candidate(
            "[task pri=1] x",
            "not-an-iso-timestamp",
            "also-bad",
            naive(2026, 5, 4, 14, 0),
            30,
        ));
    }

    // ------------- format_heartbeat_hint -------------

    #[test]
    fn format_returns_empty_for_no_titles() {
        assert_eq!(format_heartbeat_hint(&[], 30), "");
    }

    #[test]
    fn format_returns_empty_when_threshold_zero() {
        let titles = vec!["x".to_string()];
        assert_eq!(format_heartbeat_hint(&titles, 0), "");
    }

    #[test]
    fn format_uses_singular_for_one_title() {
        let titles = vec!["整理 Downloads".to_string()];
        let s = format_heartbeat_hint(&titles, 30);
        assert!(s.contains("[心跳]"));
        assert!(s.contains("「整理 Downloads」"));
        assert!(s.contains("30 分钟"));
        // 没有列表项 marker
        assert!(!s.contains("\n- "));
        // 包含动作指令
        assert!(s.contains("memory_edit update"));
        assert!(s.contains("[done]"));
    }

    #[test]
    fn format_lists_multiple_titles_with_bullets() {
        let titles = vec!["A".to_string(), "B".to_string(), "C".to_string()];
        let s = format_heartbeat_hint(&titles, 45);
        assert!(s.contains("[心跳]"));
        assert!(s.contains("45 分钟"));
        assert!(s.contains("- A"));
        assert!(s.contains("- B"));
        assert!(s.contains("- C"));
        assert!(s.contains("memory_edit update"));
    }
}
