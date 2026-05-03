//! Pure time/calendar/idle-band helpers (Iter QG5c-prep extraction from `proactive.rs`).
//!
//! All functions here are zero-state, no-IO, deterministic mappings from
//! primitive inputs to strings or bools. They're used in three places:
//! 1. Prompt assembly (period label, weekday hint, idle/absence registers).
//! 2. The gate (in_quiet_hours, minutes_until_quiet_start).
//! 3. ToneSnapshot (period_of_day for the panel chip).
//!
//! Public surface preserved via `pub use self::time_helpers::*` at the top
//! of `proactive.rs` so existing callsites (mostly bare-name invocations
//! within proactive's own scope) keep working.

/// Map an elapsed-minutes count (since the pet last spoke proactively) to a Chinese
/// "cadence" label. Lets the LLM shift register from "continuing a thread" through
/// "checking back in" to "haven't talked in ages" without doing the math itself.
/// Boundaries are conversational, not strict — 16 minutes is still "聊过一会儿".
pub fn idle_tier(minutes: u64) -> &'static str {
    match minutes {
        0..=15 => "刚说过话，话题还热",
        16..=60 => "聊过一会儿了",
        61..=360 => "几小时没说话",
        361..=1440 => "已经隔了大半天",
        _ => "上次聊已经是昨天或更早",
    }
}

/// Iter Cμ: map idle_minutes (since the *user* last interacted with the pet) into
/// a register cue distinct from `idle_tier`. The pet's cadence ("我刚说过话")
/// vs user absence ("用户刚走开几小时") are different axes — the prompt benefits
/// from both. Used in the time line so the LLM can lean into "终于回来了" /
/// "想你了一下" registers when warranted, instead of treating 5 minutes and 5 hours
/// of absence the same way.
pub fn user_absence_tier(idle_minutes: u64) -> &'static str {
    match idle_minutes {
        0..=15 => "用户刚刚还在",
        16..=60 => "用户离开了一小会儿",
        61..=180 => "用户走开有一两小时了",
        181..=480 => "用户已经离开了大半天",
        481..=1440 => "用户一整天没出现",
        _ => "用户至少一天没和你互动",
    }
}

/// Map a 24-hour clock value (0–23) to a Chinese period-of-day label. Used in the
/// proactive prompt so the LLM can riff on time-of-day vibes ("早上的咖啡时间到了") rather
/// than just seeing a numeric timestamp. Boundaries match common Chinese conversational
/// usage; the function is `pub` so tests can pin them.
pub fn period_of_day(hour: u8) -> &'static str {
    match hour {
        5..=7 => "清晨",
        8..=10 => "上午",
        11..=12 => "中午",
        13..=16 => "下午",
        17..=18 => "傍晚",
        19..=21 => "晚上",
        _ => "深夜", // 22, 23, 0..=4
    }
}

/// Chinese label for a weekday. Used by `format_day_of_week_hint` so the LLM sees
/// "今天是周X（工作日/周末）" instead of just date arithmetic; weekday vs weekend
/// shifts what topics make sense (Friday-night slack vs Monday-morning ramp-up).
pub fn weekday_zh(wd: chrono::Weekday) -> &'static str {
    use chrono::Weekday::*;
    match wd {
        Mon => "周一",
        Tue => "周二",
        Wed => "周三",
        Thu => "周四",
        Fri => "周五",
        Sat => "周六",
        Sun => "周日",
    }
}

/// "周末" (Sat / Sun) vs "工作日" (Mon–Fri). Distinct from `weekday_zh` because the
/// prompt phrases both — the LLM benefits from being told the category explicitly
/// instead of inferring it from "周六" alone (less robust across model versions).
pub fn weekday_kind_zh(wd: chrono::Weekday) -> &'static str {
    use chrono::Weekday::*;
    match wd {
        Sat | Sun => "周末",
        _ => "工作日",
    }
}

/// Format the combined day-of-week hint that the proactive time line embeds.
/// Pure for testability — the `run_proactive_turn` callsite passes `now_local.weekday()`.
/// Output example: "周日 · 周末" / "周二 · 工作日". Joined by `·` to read naturally
/// when concatenated into "（下午，周二 · 工作日）".
pub fn format_day_of_week_hint(wd: chrono::Weekday) -> String {
    format!("{} · {}", weekday_zh(wd), weekday_kind_zh(wd))
}

/// How many minutes until the next quiet-hours boundary, when that boundary is within
/// `look_ahead_minutes`. Returns `None` when:
/// - quiet hours are disabled (start == end)
/// - we're already inside the quiet window (then there's nothing to "approach")
/// - the boundary is more than `look_ahead_minutes` away
///
/// Used to inject a "winding down for the night" rule into the prompt so the pet eases
/// into the quiet window with a gentler tone instead of going from full chatter to a
/// hard silent gate. Pure so tests can pin every interesting (now, start) combination.
pub fn minutes_until_quiet_start(
    now_hour: u8,
    now_minute: u8,
    quiet_start: u8,
    quiet_end: u8,
    look_ahead_minutes: u64,
) -> Option<u64> {
    if quiet_start == quiet_end {
        return None;
    }
    if in_quiet_hours(now_hour, quiet_start, quiet_end) {
        return None;
    }
    let now_total = now_hour as i32 * 60 + now_minute as i32;
    let start_total = quiet_start as i32 * 60;
    let mut delta = start_total - now_total;
    if delta < 0 {
        delta += 24 * 60; // next day's quiet_start
    }
    let delta_u = delta as u64;
    if delta_u <= look_ahead_minutes {
        Some(delta_u)
    } else {
        None
    }
}

/// Returns true if `hour` (0–23) falls inside the quiet window `[start, end)`. Handles the
/// midnight wrap-around case (start > end, e.g. 23:00–07:00). When start == end, the gate
/// is treated as disabled (no quiet hours configured). Iter D4: now `pub` so
/// `get_tone_snapshot` can surface "is the pet currently dormant" to the panel.
pub fn in_quiet_hours(hour: u8, start: u8, end: u8) -> bool {
    if start == end {
        return false;
    }
    if start < end {
        // Same-day window, e.g. 13–15.
        hour >= start && hour < end
    } else {
        // Wraps past midnight, e.g. 23–7. In quiet if hour >= 23 OR hour < 7.
        hour >= start || hour < end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- idle_tier ----

    #[test]
    fn idle_tier_each_tier_has_a_representative_minute() {
        assert_eq!(idle_tier(0), "刚说过话，话题还热");
        assert_eq!(idle_tier(8), "刚说过话，话题还热");
        assert_eq!(idle_tier(30), "聊过一会儿了");
        assert_eq!(idle_tier(120), "几小时没说话");
        assert_eq!(idle_tier(720), "已经隔了大半天");
        assert_eq!(idle_tier(2000), "上次聊已经是昨天或更早");
    }

    #[test]
    fn idle_tier_boundaries_land_on_expected_side() {
        assert_eq!(idle_tier(15), "刚说过话，话题还热");
        assert_eq!(idle_tier(16), "聊过一会儿了");
        assert_eq!(idle_tier(60), "聊过一会儿了");
        assert_eq!(idle_tier(61), "几小时没说话");
        assert_eq!(idle_tier(360), "几小时没说话");
        assert_eq!(idle_tier(361), "已经隔了大半天");
        assert_eq!(idle_tier(1440), "已经隔了大半天");
        assert_eq!(idle_tier(1441), "上次聊已经是昨天或更早");
    }

    // ---- user_absence_tier ----

    #[test]
    fn user_absence_tier_maps_each_band() {
        assert_eq!(user_absence_tier(0), "用户刚刚还在");
        assert_eq!(user_absence_tier(15), "用户刚刚还在");
        assert_eq!(user_absence_tier(16), "用户离开了一小会儿");
        assert_eq!(user_absence_tier(60), "用户离开了一小会儿");
        assert_eq!(user_absence_tier(61), "用户走开有一两小时了");
        assert_eq!(user_absence_tier(180), "用户走开有一两小时了");
        assert_eq!(user_absence_tier(181), "用户已经离开了大半天");
        assert_eq!(user_absence_tier(480), "用户已经离开了大半天");
        assert_eq!(user_absence_tier(481), "用户一整天没出现");
        assert_eq!(user_absence_tier(1440), "用户一整天没出现");
        assert_eq!(user_absence_tier(1441), "用户至少一天没和你互动");
    }

    // ---- period_of_day ----

    #[test]
    fn period_each_bucket_has_a_representative_hour() {
        assert_eq!(period_of_day(6), "清晨");
        assert_eq!(period_of_day(9), "上午");
        assert_eq!(period_of_day(12), "中午");
        assert_eq!(period_of_day(15), "下午");
        assert_eq!(period_of_day(18), "傍晚");
        assert_eq!(period_of_day(20), "晚上");
        assert_eq!(period_of_day(23), "深夜");
        assert_eq!(period_of_day(2), "深夜");
    }

    #[test]
    fn period_boundaries_land_on_expected_side() {
        assert_eq!(period_of_day(4), "深夜");
        assert_eq!(period_of_day(5), "清晨");
        assert_eq!(period_of_day(7), "清晨");
        assert_eq!(period_of_day(8), "上午");
        assert_eq!(period_of_day(10), "上午");
        assert_eq!(period_of_day(11), "中午");
        assert_eq!(period_of_day(13), "下午");
        assert_eq!(period_of_day(16), "下午");
        assert_eq!(period_of_day(17), "傍晚");
        assert_eq!(period_of_day(19), "晚上");
        assert_eq!(period_of_day(21), "晚上");
        assert_eq!(period_of_day(22), "深夜");
        assert_eq!(period_of_day(0), "深夜");
    }

    // ---- weekday helpers ----

    #[test]
    fn weekday_zh_maps_each_weekday() {
        use chrono::Weekday::*;
        assert_eq!(weekday_zh(Mon), "周一");
        assert_eq!(weekday_zh(Tue), "周二");
        assert_eq!(weekday_zh(Wed), "周三");
        assert_eq!(weekday_zh(Thu), "周四");
        assert_eq!(weekday_zh(Fri), "周五");
        assert_eq!(weekday_zh(Sat), "周六");
        assert_eq!(weekday_zh(Sun), "周日");
    }

    #[test]
    fn weekday_kind_zh_distinguishes_weekend() {
        use chrono::Weekday::*;
        assert_eq!(weekday_kind_zh(Mon), "工作日");
        assert_eq!(weekday_kind_zh(Tue), "工作日");
        assert_eq!(weekday_kind_zh(Fri), "工作日");
        assert_eq!(weekday_kind_zh(Sat), "周末");
        assert_eq!(weekday_kind_zh(Sun), "周末");
    }

    #[test]
    fn format_day_of_week_hint_combines_label_and_kind() {
        use chrono::Weekday::*;
        assert_eq!(format_day_of_week_hint(Sun), "周日 · 周末");
        assert_eq!(format_day_of_week_hint(Wed), "周三 · 工作日");
    }

    // ---- minutes_until_quiet_start ----

    #[test]
    fn pre_quiet_within_window_returns_minutes() {
        assert_eq!(minutes_until_quiet_start(22, 50, 23, 7, 15), Some(10));
    }

    #[test]
    fn pre_quiet_at_window_edge_15_min() {
        assert_eq!(minutes_until_quiet_start(22, 45, 23, 7, 15), Some(15));
    }

    #[test]
    fn pre_quiet_outside_window_returns_none() {
        assert_eq!(minutes_until_quiet_start(22, 44, 23, 7, 15), None);
    }

    #[test]
    fn pre_quiet_already_in_quiet_returns_none() {
        assert_eq!(minutes_until_quiet_start(3, 0, 23, 7, 15), None);
        assert_eq!(minutes_until_quiet_start(23, 30, 23, 7, 15), None);
    }

    #[test]
    fn pre_quiet_disabled_when_start_equals_end() {
        assert_eq!(minutes_until_quiet_start(22, 50, 0, 0, 15), None);
    }

    #[test]
    fn pre_quiet_same_day_window() {
        assert_eq!(minutes_until_quiet_start(13, 55, 14, 15, 15), Some(5));
    }

    #[test]
    fn pre_quiet_past_today_uses_tomorrow() {
        assert_eq!(minutes_until_quiet_start(7, 0, 23, 7, 15), None);
    }

    // ---- in_quiet_hours ----

    #[test]
    fn quiet_hours_disabled_when_start_equals_end() {
        assert!(!in_quiet_hours(0, 0, 0));
        assert!(!in_quiet_hours(12, 23, 23));
    }

    #[test]
    fn quiet_hours_same_day_window() {
        assert!(in_quiet_hours(14, 13, 15));
        assert!(!in_quiet_hours(15, 13, 15)); // end is exclusive
        assert!(!in_quiet_hours(12, 13, 15));
    }

    #[test]
    fn quiet_hours_wraps_past_midnight() {
        // 23:00 → 7:00 quiet window
        assert!(in_quiet_hours(23, 23, 7));
        assert!(in_quiet_hours(0, 23, 7));
        assert!(in_quiet_hours(6, 23, 7));
        assert!(!in_quiet_hours(7, 23, 7)); // end exclusive
        assert!(!in_quiet_hours(12, 23, 7));
    }
}
