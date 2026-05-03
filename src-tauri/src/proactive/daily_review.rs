//! Iter R12: end-of-day reflection. At 22:00 (or the first proactive tick
//! after) the pet writes a `daily_review_YYYY-MM-DD` entry into the
//! `ai_insights` memory category — captures today's proactive speeches and
//! the active daily_plan so the next day's first turn can read "what we did
//! together yesterday" rather than starting cold.
//!
//! The gate (`should_trigger_daily_review`) and the body formatter
//! (`format_daily_review_detail`) are pure / testable. The async wrapper
//! that touches memory + speech_history lives in proactive.rs so this
//! module stays clock-and-fs independent.
//!
//! Currently *deterministic* — bullet list of today's speeches, no LLM
//! summary. R12b will follow up with an LLM "today we together..." line.

use chrono::NaiveDate;

/// First hour at which the daily review fires. After this hour, the next
/// proactive turn whose `LAST_DAILY_REVIEW_DATE` doesn't equal today runs
/// the review once. 22:00 ≈ "winding-down" — late enough that most of the
/// day's speeches have happened, early enough that the user is likely
/// still around to see the next-morning callback.
pub const DAILY_REVIEW_HOUR: u8 = 22;

/// Process-wide last-fire date. `None` on fresh process start. Cross-restart
/// idempotency is layered on top by checking the actual memory index for
/// the title before writing — this static is only the in-session fast path.
pub static LAST_DAILY_REVIEW_DATE: std::sync::Mutex<Option<NaiveDate>> =
    std::sync::Mutex::new(None);

/// Pure gate. Returns true iff `now_hour ≥ DAILY_REVIEW_HOUR` AND we
/// haven't yet reviewed today. Called every proactive tick — first tick
/// after 22:00 wins; subsequent ticks see `last == today` and skip.
pub fn should_trigger_daily_review(
    now_hour: u8,
    today: NaiveDate,
    last_review_date: Option<NaiveDate>,
) -> bool {
    if now_hour < DAILY_REVIEW_HOUR {
        return false;
    }
    !matches!(last_review_date, Some(d) if d == today)
}

/// Pure formatter for the markdown body written to the detail .md file.
/// `speeches` is already redacted + timestamp-stripped at call site;
/// `plan_description` is the raw description text from `ai_insights/daily_plan`.
/// Empty plan / empty speeches are handled with explicit "no entries" notes
/// so the artifact is self-explanatory rather than silently truncated.
pub fn format_daily_review_detail(
    speeches: &[String],
    plan_description: &str,
    date: NaiveDate,
) -> String {
    let mut out = format!("# 今日回顾 — {}\n\n", date);
    out.push_str("## 今日计划\n");
    if plan_description.trim().is_empty() {
        out.push_str("（今天没有定计划。）\n\n");
    } else {
        out.push_str(plan_description.trim());
        out.push_str("\n\n");
    }
    out.push_str("## 主动开口记录\n");
    if speeches.is_empty() {
        out.push_str("（今天没有主动开过口。）\n");
    } else {
        for line in speeches {
            out.push_str(&format!("- {}\n", line));
        }
    }
    out
}

/// Short one-line index description, surfaces in the panel memory list.
/// Leading `[review]` lets a future R12b LLM-summary pass identify and
/// upgrade these entries without overwriting non-review items.
pub fn format_daily_review_description(speech_count: usize, has_plan: bool) -> String {
    let plan_part = if has_plan { "，有计划" } else { "" };
    format!("[review] 今天主动开口 {} 次{}", speech_count, plan_part)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn gate_blocks_before_22() {
        let today = d(2026, 5, 3);
        assert!(!should_trigger_daily_review(0, today, None));
        assert!(!should_trigger_daily_review(12, today, None));
        assert!(!should_trigger_daily_review(21, today, None));
    }

    #[test]
    fn gate_fires_at_22_with_no_prior() {
        let today = d(2026, 5, 3);
        assert!(should_trigger_daily_review(22, today, None));
    }

    #[test]
    fn gate_fires_at_23_with_no_prior() {
        let today = d(2026, 5, 3);
        assert!(should_trigger_daily_review(23, today, None));
    }

    #[test]
    fn gate_blocks_when_already_reviewed_today() {
        let today = d(2026, 5, 3);
        assert!(!should_trigger_daily_review(22, today, Some(today)));
        assert!(!should_trigger_daily_review(23, today, Some(today)));
    }

    #[test]
    fn gate_fires_when_last_review_was_yesterday() {
        let today = d(2026, 5, 3);
        let yesterday = d(2026, 5, 2);
        assert!(should_trigger_daily_review(22, today, Some(yesterday)));
    }

    #[test]
    fn gate_blocks_at_21_even_after_old_review() {
        let today = d(2026, 5, 3);
        let yesterday = d(2026, 5, 2);
        // Hour < 22 means "not yet review time" — old review state doesn't
        // reopen the gate retroactively.
        assert!(!should_trigger_daily_review(21, today, Some(yesterday)));
    }

    #[test]
    fn detail_renders_full_body_with_plan_and_speeches() {
        let speeches = vec!["早上好".to_string(), "午饭吃了吗".to_string()];
        let plan = "· 关心工作进展 [1/2]";
        let body = format_daily_review_detail(&speeches, plan, d(2026, 5, 3));
        assert!(body.contains("# 今日回顾 — 2026-05-03"));
        assert!(body.contains("## 今日计划"));
        assert!(body.contains("· 关心工作进展 [1/2]"));
        assert!(body.contains("## 主动开口记录"));
        assert!(body.contains("- 早上好"));
        assert!(body.contains("- 午饭吃了吗"));
    }

    #[test]
    fn detail_notes_empty_plan() {
        let speeches = vec!["你好".to_string()];
        let body = format_daily_review_detail(&speeches, "", d(2026, 5, 3));
        assert!(body.contains("（今天没有定计划。）"));
        assert!(body.contains("- 你好"));
    }

    #[test]
    fn detail_notes_empty_speeches() {
        let body = format_daily_review_detail(&[], "· 提醒喝水 [0/1]", d(2026, 5, 3));
        assert!(body.contains("· 提醒喝水 [0/1]"));
        assert!(body.contains("（今天没有主动开过口。）"));
    }

    #[test]
    fn detail_notes_both_empty() {
        let body = format_daily_review_detail(&[], "   ", d(2026, 5, 3));
        assert!(body.contains("（今天没有定计划。）"));
        assert!(body.contains("（今天没有主动开过口。）"));
    }

    #[test]
    fn description_records_count_and_plan_flag() {
        assert_eq!(
            format_daily_review_description(0, false),
            "[review] 今天主动开口 0 次"
        );
        assert_eq!(
            format_daily_review_description(7, true),
            "[review] 今天主动开口 7 次，有计划"
        );
        assert_eq!(
            format_daily_review_description(15, false),
            "[review] 今天主动开口 15 次"
        );
    }
}
