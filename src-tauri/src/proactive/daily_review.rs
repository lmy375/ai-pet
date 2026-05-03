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
//! Iter R12b refines the description with parsed plan progress markers
//! `[N/M]` so the panel index reads "今天主动开口 7 次，计划 3/5" instead
//! of the vague "有计划". Still deterministic — LLM-summary upgrade
//! deferred to a later iter (would require routing AppHandle + chat
//! pipeline through what's currently a clock-pure module).

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

/// Iter R12b: parse `[N/M]` progress markers out of a daily_plan description
/// and sum them into a (completed, total) tuple. Returns `None` when no
/// well-formed markers exist (e.g. plan with free-text bullets, no plan).
/// `M == 0` markers are skipped (degenerate "0 of 0" carries no info and
/// would crash a "X/Y" formatter that expects total > 0).
///
/// Examples:
/// - `"· 关心工作 [1/2]\n· 提醒喝水 [0/1]"` → `Some((1, 3))`
/// - `"· 不带 marker"` → `None`
/// - `""` → `None`
/// - `"· bad [a/b]"` → `None` (no valid markers)
pub fn parse_plan_progress(plan_description: &str) -> Option<(u32, u32)> {
    let mut completed: u32 = 0;
    let mut total: u32 = 0;
    let mut found_any = false;
    let bytes = plan_description.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'[' {
            i += 1;
            continue;
        }
        let Some(close_offset) = bytes[i + 1..].iter().position(|&b| b == b']') else {
            break;
        };
        let inner = &plan_description[i + 1..i + 1 + close_offset];
        i += 2 + close_offset;
        if let Some((c_str, t_str)) = inner.split_once('/') {
            let c_trim = c_str.trim();
            let t_trim = t_str.trim();
            // Reject markers with a non-digit prefix (e.g. "remind:") that
            // happen to contain a slash — those are reminder/butler schedule
            // tags, not progress trackers.
            if !c_trim.chars().all(|ch| ch.is_ascii_digit())
                || !t_trim.chars().all(|ch| ch.is_ascii_digit())
                || c_trim.is_empty()
                || t_trim.is_empty()
            {
                continue;
            }
            let (Ok(c), Ok(t)) = (c_trim.parse::<u32>(), t_trim.parse::<u32>()) else {
                continue;
            };
            if t == 0 {
                continue;
            }
            completed = completed.saturating_add(c);
            total = total.saturating_add(t);
            found_any = true;
        }
    }
    if found_any {
        Some((completed, total))
    } else {
        None
    }
}

/// Short one-line index description, surfaces in the panel memory list.
/// Leading `[review]` lets a future LLM-summary pass identify and upgrade
/// these entries without overwriting non-review items.
///
/// Iter R12b: when the plan has parseable `[N/M]` markers, suffix becomes
/// `，计划 N/M` (concrete). Falls back to `，有计划` only when plan exists
/// but has no parseable markers (free-text plan), and to no suffix when
/// there's no plan at all.
pub fn format_daily_review_description(
    speech_count: usize,
    plan_progress: Option<(u32, u32)>,
    has_plan: bool,
) -> String {
    let plan_part = match plan_progress {
        Some((c, t)) => format!("，计划 {}/{}", c, t),
        None if has_plan => "，有计划".to_string(),
        None => String::new(),
    };
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
            format_daily_review_description(0, None, false),
            "[review] 今天主动开口 0 次"
        );
        assert_eq!(
            format_daily_review_description(7, None, true),
            "[review] 今天主动开口 7 次，有计划"
        );
        assert_eq!(
            format_daily_review_description(15, None, false),
            "[review] 今天主动开口 15 次"
        );
    }

    #[test]
    fn description_shows_concrete_plan_progress_when_parseable() {
        // R12b: progress markers take precedence over the vague "有计划" suffix.
        assert_eq!(
            format_daily_review_description(7, Some((1, 3)), true),
            "[review] 今天主动开口 7 次，计划 1/3"
        );
        assert_eq!(
            format_daily_review_description(0, Some((0, 5)), true),
            "[review] 今天主动开口 0 次，计划 0/5"
        );
        // has_plan true but progress None → fall back to "有计划".
        assert_eq!(
            format_daily_review_description(3, None, true),
            "[review] 今天主动开口 3 次，有计划"
        );
    }

    #[test]
    fn parse_progress_sums_multiple_markers() {
        let plan = "· 关心工作 [1/2]\n· 提醒喝水 [0/1]\n· 早安问候 [1/1]";
        assert_eq!(parse_plan_progress(plan), Some((2, 4)));
    }

    #[test]
    fn parse_progress_handles_single_marker() {
        assert_eq!(parse_plan_progress("· task [3/5]"), Some((3, 5)));
    }

    #[test]
    fn parse_progress_returns_none_for_no_markers() {
        assert_eq!(parse_plan_progress(""), None);
        assert_eq!(parse_plan_progress("· 自由形式的计划"), None);
        assert_eq!(parse_plan_progress("没有方括号的内容"), None);
    }

    #[test]
    fn parse_progress_skips_malformed_markers() {
        // [a/b] non-digit, [10] no slash, [/3] empty left, [3/] empty right
        // — none should count, expected None.
        let plan = "· bad [a/b]\n· nope [10]\n· empty [/3]\n· empty2 [3/]";
        assert_eq!(parse_plan_progress(plan), None);
    }

    #[test]
    fn parse_progress_skips_marker_with_zero_total() {
        // [1/0] is degenerate — would mean "1 of 0" which can't be displayed
        // sensibly. Skip but don't fail the whole parse.
        assert_eq!(parse_plan_progress("· bad [1/0]"), None);
        // Still pick up valid neighbors.
        assert_eq!(
            parse_plan_progress("· good [2/3]\n· bad [1/0]"),
            Some((2, 3))
        );
    }

    #[test]
    fn parse_progress_ignores_non_progress_brackets() {
        // R12b: reminder / schedule prefix tags use [HH:MM] / [remind: ...] /
        // [every: ...] — they contain colons, no digit/slash/digit, so they
        // shouldn't crash parsing or contribute to progress.
        let plan = "· [remind: 09:00] 喝水 [0/1]\n· [every: 18:00] 运动 [1/1]";
        assert_eq!(parse_plan_progress(plan), Some((1, 2)));
    }

    #[test]
    fn parse_progress_handles_whitespace_inside_marker() {
        // Be lenient about [ 1 / 2 ] style — humans type with spaces.
        assert_eq!(parse_plan_progress("· task [ 1 / 2 ]"), Some((1, 2)));
    }
}
