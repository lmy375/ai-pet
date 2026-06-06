/// `/due` 命令的 preset 维度。绑定 caller 的 "今天" 后展开为具体 date
/// range（pure formatter 内做，避免 parser 拿运行时时间，便于单测）。
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum DuePreset {
    /// 明天：today + 1 day。
    Tomorrow,
    /// 本周：包含 today 在内的 Mon..=Sun（ISO 周）。已过去的工作日仍算
    /// 在内（owner 想 audit "本周还剩什么 due"），由 formatter 加 hint。
    ThisWeek,
    /// 下周：本周 Sun 之后的 Mon..=Sun。
    NextWeek,
}

/// pure：识别 owner 输入的 preset 字符串。中英 alias 同表；大小写不敏感。
/// 未识别返 None 让 handler 走 usage hint。
pub fn parse_due_preset(s: &str) -> Option<DuePreset> {
    let lower = s.trim().to_lowercase();
    match lower.as_str() {
        "tomorrow" | "tmr" | "tm" | "明天" | "明日" => Some(DuePreset::Tomorrow),
        "thisweek" | "this-week" | "this_week" | "week" | "本周" | "这周" => {
            Some(DuePreset::ThisWeek)
        }
        "nextweek" | "next-week" | "next_week" | "下周" => Some(DuePreset::NextWeek),
        _ => None,
    }
}

/// iter #393: `/edit_due <title> <preset>` 命令的 preset 维度。比
/// `/due` 的 DuePreset（仅 audit 时间段）更广 — 含 tonight / 单
/// weekday / next-week weekday / +Nm/h/d 相对时长 / clear 多形态。
/// caller 把 preset 与 now 注入 `compute_edit_due_preset` 得到具体
/// NaiveDateTime（pure，便单测）。
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum EditDuePreset {
    /// today 18:00；若 now 已过 18:00 → tomorrow 18:00（避免点完一下
    /// 子 "tonight" 又被解释成已过去时刻 footgun）
    Tonight,
    /// tomorrow 09:00
    TomorrowMorning,
    /// 本周（或最近未来）某 weekday 09:00。`weekday`: 0=Mon..6=Sun
    /// 与 chrono::Weekday::num_days_from_monday() 同 mapping
    Weekday(u8),
    /// 下周某 weekday 09:00（本周已过或本日 weekday 也算下周以避免
    /// 撞当日 footgun）
    NextWeekday(u8),
    /// now + minutes（+Nm）
    PlusMinutes(u32),
    /// now + hours（+Nh）
    PlusHours(u32),
    /// now + days 09:00（+Nd — 几天后早上 9 点，而非"几天后此刻"避
    /// 免 due 落到午夜 / 半夜的反直觉）
    PlusDays(u32),
    /// 清掉 due（"clear" / "none" / "0"）
    Clear,
}

/// pure：识别 owner 输入的 edit_due preset。tonight / morning / 单
/// weekday / next-week weekday / +Nm/h/d / clear 多形态；中英 alias
/// 同表；大小写不敏感。未识别返 None 让 handler 走 usage hint。
pub fn parse_edit_due_preset(s: &str) -> Option<EditDuePreset> {
    let lower = s.trim().to_lowercase();
    match lower.as_str() {
        "tonight" | "今晚" | "today_evening" | "today-evening" => {
            return Some(EditDuePreset::Tonight);
        }
        "tomorrow" | "tmr" | "tm" | "明天" | "明日" | "morning" | "早上" => {
            return Some(EditDuePreset::TomorrowMorning);
        }
        "clear" | "none" | "0" | "清除" | "取消" => {
            return Some(EditDuePreset::Clear);
        }
        _ => {}
    }
    // Weekday 单词：mon/tue/.../sun + 周一..周日
    let weekday_map: &[(&str, u8)] = &[
        ("monday", 0), ("mon", 0), ("周一", 0), ("星期一", 0),
        ("tuesday", 1), ("tue", 1), ("周二", 1), ("星期二", 1),
        ("wednesday", 2), ("wed", 2), ("周三", 2), ("星期三", 2),
        ("thursday", 3), ("thu", 3), ("周四", 3), ("星期四", 3),
        ("friday", 4), ("fri", 4), ("周五", 4), ("星期五", 4),
        ("saturday", 5), ("sat", 5), ("周六", 5), ("星期六", 5),
        ("sunday", 6), ("sun", 6), ("周日", 6), ("周天", 6), ("星期日", 6),
    ];
    for (alias, idx) in weekday_map {
        if lower == *alias {
            return Some(EditDuePreset::Weekday(*idx));
        }
        // next_<weekday> / next-mon / 下周一
        let next_prefixes = ["next_", "next-", "下"];
        for pfx in &next_prefixes {
            let key = format!("{}{}", pfx, alias);
            if lower == key {
                return Some(EditDuePreset::NextWeekday(*idx));
            }
        }
    }
    // 相对时长：+Nm / +Nh / +Nd
    if let Some(rest) = lower.strip_prefix('+') {
        let (digits, unit) = rest.split_at(rest.len().saturating_sub(1));
        if let Ok(n) = digits.parse::<u32>() {
            if n > 0 {
                return match unit {
                    "m" => Some(EditDuePreset::PlusMinutes(n)),
                    "h" => Some(EditDuePreset::PlusHours(n)),
                    "d" => Some(EditDuePreset::PlusDays(n)),
                    _ => None,
                };
            }
        }
    }
    None
}

/// pure：把 EditDuePreset + now 算出具体 NaiveDateTime。`None` = Clear
/// 语义（caller 传 None 给 task_set_due 清 due）；`Some(dt)` = 设
/// 该时刻。返回类型 `Option<Option<NaiveDateTime>>` 似乎冗余，但语义
/// 上 `Some(None)` 是 "明确 Clear"（不是错误），与 `Some(Some(dt))`
/// 区分；caller 把内层 Option 转 `Option<String>` 传给 task_set_due。
pub fn compute_edit_due_preset(
    preset: &EditDuePreset,
    now: chrono::NaiveDateTime,
) -> Option<chrono::NaiveDateTime> {
    use chrono::{Duration, NaiveTime};
    let today = now.date();
    let nine_am = NaiveTime::from_hms_opt(9, 0, 0)?;
    let six_pm = NaiveTime::from_hms_opt(18, 0, 0)?;
    match preset {
        EditDuePreset::Tonight => {
            let tonight = today.and_time(six_pm);
            if tonight > now {
                Some(tonight)
            } else {
                // 已过 18:00 → 明晚 18:00 防"tonight 已过去"footgun
                Some((today + Duration::days(1)).and_time(six_pm))
            }
        }
        EditDuePreset::TomorrowMorning => {
            Some((today + Duration::days(1)).and_time(nine_am))
        }
        EditDuePreset::Weekday(idx) => {
            // 当前 weekday → target weekday 之差（mod 7）；0 时算下周
            // （避免设到今天同 weekday 但当前已过 9 点 → 落已过时刻）
            use chrono::Datelike;
            let cur = today.weekday().num_days_from_monday() as i64;
            let target = *idx as i64;
            let mut diff = (target - cur).rem_euclid(7);
            if diff == 0 {
                // 当日 weekday：若 09:00 仍未来则当日，否则下周
                let target_today = today.and_time(nine_am);
                if target_today > now {
                    return Some(target_today);
                }
                diff = 7;
            }
            Some((today + Duration::days(diff)).and_time(nine_am))
        }
        EditDuePreset::NextWeekday(idx) => {
            use chrono::Datelike;
            let cur = today.weekday().num_days_from_monday() as i64;
            let target = *idx as i64;
            let base_diff = (target - cur).rem_euclid(7);
            // 显式 "next" 语义：至少 7 天之后（即使 base_diff > 0）
            let diff = if base_diff == 0 { 7 } else { base_diff + 7 };
            Some((today + Duration::days(diff)).and_time(nine_am))
        }
        EditDuePreset::PlusMinutes(n) => Some(now + Duration::minutes(*n as i64)),
        EditDuePreset::PlusHours(n) => Some(now + Duration::hours(*n as i64)),
        EditDuePreset::PlusDays(n) => {
            Some((today + Duration::days(*n as i64)).and_time(nine_am))
        }
        EditDuePreset::Clear => None,
    }
}
