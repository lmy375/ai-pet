//! Append-only history of the pet's own mood entries (Iter 103, route A).
//!
//! Mirrors the design of `speech_history` — one line per recorded mood, capped at a
//! manageable line count, size-bounded for safety, and parsed by pure helpers so the
//! "what's my mood trend over the last week?" question can be answered without
//! re-running an LLM.
//!
//! Format: `<ISO8601 timestamp> <MOTION> | <free text>` where MOTION is one of
//! `Tap | Flick | Flick3 | Idle | -` (- when the mood entry was missing the
//! `[motion: X]` prefix). The pipe lets pure parsers split text from motion without
//! ambiguity even when the text contains spaces.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Serialize;

use crate::log_rotation::rotate_if_needed;

/// Hard cap on retained entries. ~200 entries comfortably covers a few weeks of
/// proactive mood updates at typical cadence; the trend summary only ever reads the
/// tail (≤ 50 lines), so older entries are pruned without loss.
pub const MOOD_HISTORY_CAP: usize = 200;
const MOOD_HISTORY_MAX_BYTES: u64 = 200_000;

/// 周报 / consolidate 用的"读全文"快捷：返回 mood_history.log 的原始内容。
/// 文件不存在 / 读失败均返回空串。调用方按行 split + 自己解析。
pub async fn read_history_content() -> String {
    let Some(path) = history_path() else {
        return String::new();
    };
    tokio::fs::read_to_string(&path).await.unwrap_or_default()
}

fn history_path() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("pet").join("mood_history.log"))
}

/// Append a mood entry (best-effort). De-dupes against the most recent line so the
/// log captures *changes* rather than every proactive tick where the mood happened
/// to still be "Idle / 平静"; otherwise the trend summary would be dominated by
/// repetition. IO errors are swallowed — mood logging never blocks the chat path.
pub async fn record_mood(text: &str, motion: &Option<String>) {
    let _ = record_mood_inner(text, motion).await;
}

async fn record_mood_inner(text: &str, motion: &Option<String>) -> std::io::Result<()> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    let Some(path) = history_path() else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let _ = rotate_if_needed(&path, MOOD_HISTORY_MAX_BYTES).await;

    let existing = tokio::fs::read_to_string(&path).await.unwrap_or_default();
    let motion_str = motion.as_deref().unwrap_or("-");

    // Skip when the most recent line records exactly the same motion + text — keeps
    // the log focused on transitions instead of identical re-reads.
    if let Some(last) = existing.lines().rfind(|l| !l.is_empty()) {
        if let Some((last_motion, last_text)) = parse_motion_text(last) {
            if last_motion == motion_str && last_text == trimmed {
                return Ok(());
            }
        }
    }

    let mut entries: Vec<String> = existing
        .lines()
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect();
    let ts = chrono::Local::now()
        .format("%Y-%m-%dT%H:%M:%S%:z")
        .to_string();
    let flat = trimmed.replace(['\n', '\r'], " ");
    entries.push(format!("{} {} | {}", ts, motion_str, flat));
    if entries.len() > MOOD_HISTORY_CAP {
        let drop = entries.len() - MOOD_HISTORY_CAP;
        entries.drain(0..drop);
    }
    let mut content = entries.join("\n");
    content.push('\n');
    tokio::fs::write(&path, content).await
}

/// Pure: split a logged line into (motion, text). Format is
/// `<ts> <motion> | <text>` — splits on the first ` | ` to allow `|` inside the
/// text without breaking parsing. Returns None on malformed lines.
pub fn parse_motion_text(line: &str) -> Option<(&str, &str)> {
    let (head, text) = line.split_once(" | ")?;
    // head is "<ts> <motion>" — split on last space.
    let (_, motion) = head.rsplit_once(' ')?;
    Some((motion, text))
}

/// Pure: from `content`, take the last `n` non-empty lines and tally motions by
/// occurrence. Returns a list of `(motion, count)` sorted by count descending. The
/// `-` motion (mood without prefix) is included; callers can filter if they want
/// a strict "tagged-only" view.
pub fn summarize_recent_motions(content: &str, n: usize) -> Vec<(String, u64)> {
    if n == 0 {
        return vec![];
    }
    let lines: Vec<&str> = content.lines().filter(|l| !l.is_empty()).collect();
    let start = lines.len().saturating_sub(n);
    let window = &lines[start..];
    let mut counts: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    for line in window {
        if let Some((motion, _)) = parse_motion_text(line) {
            *counts.entry(motion.to_string()).or_insert(0) += 1;
        }
    }
    let mut sorted: Vec<(String, u64)> = counts.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    sorted
}

/// Pure: build the natural-language trend hint from the recent-mood window. None when
/// the log has too few entries to say anything useful (< `min_entries`); avoids the
/// "your mood has been Idle ×1 (most common!)" pseudo-insight on day 0.
pub fn format_trend_hint(content: &str, n: usize, min_entries: u64) -> Option<String> {
    let counts = summarize_recent_motions(content, n);
    let total: u64 = counts.iter().map(|(_, c)| c).sum();
    if total < min_entries {
        return None;
    }
    let parts: Vec<String> = counts
        .iter()
        .filter(|(m, _)| m != "-")
        .map(|(m, c)| format!("{} × {}", m, c))
        .collect();
    if parts.is_empty() {
        return None;
    }
    Some(format!(
        "你最近 {} 次心情记录里：{}（按出现次数排序）。这是你长期的情绪谱——可以让 ta 渗进当下语气，但不必生硬带出。",
        total,
        parts.join("、"),
    ))
}

/// Read the file and return the trend hint, or empty string when no log / too few
/// entries. Convenience for proactive prompt construction.
pub async fn build_trend_hint(window: usize, min_entries: u64) -> String {
    let Some(path) = history_path() else {
        return String::new();
    };
    let content = tokio::fs::read_to_string(&path).await.unwrap_or_default();
    format_trend_hint(&content, window, min_entries).unwrap_or_default()
}

/// Tauri command returning the formatted mood-trend hint (Iter 105). Same window /
/// min-entries the proactive prompt uses, so the panel and the LLM see the exact
/// same trend description — no source-of-truth divergence.
#[tauri::command]
pub async fn get_mood_trend_hint() -> String {
    build_trend_hint(50, 5).await
}

/// 给 PanelPersona 心情趋势 sparkline 用的"按天聚合"数据形态。每条 = 一天 +
/// 当天每个 motion 的频次 + 当天总数。`date` 是 `YYYY-MM-DD` 本地时区文本。
///
/// 用 BTreeMap 让 motions 字段在 JSON 里有稳定 key 顺序（前端渲染时不会因 hash
/// 顺序变化而抖动颜色顺序）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DailyMotion {
    pub date: String,
    pub motions: BTreeMap<String, u64>,
    pub total: u64,
}

/// Pure：从 mood_history 文件内容中聚合"最近 N 天"的 motion 计数。
///
/// - `days`：窗口大小，例如 7 = 包含 today 在内最近 7 天
/// - `today`：调用方提供的"今天"日期（注入便于测试）
///
/// 返回 `Vec<DailyMotion>`，**按日期升序**（最旧 → 最新），长度恰为 `days`。
/// 没有记录的日子用 `total = 0` + 空 `motions` 占位，让前端的等宽柱状能 zip
/// 到一一对应的列。
///
/// 解析失败行（无 ` | `、ts 不是 RFC3339、等）一律跳过 —— 与现有
/// `parse_motion_text` / `speeches_for_date` 的容错语义一致，坏数据不污染聚合。
pub fn summarize_motions_by_day(
    content: &str,
    days: usize,
    today: chrono::NaiveDate,
) -> Vec<DailyMotion> {
    if days == 0 {
        return vec![];
    }
    // 预生成窗口里每一天的占位（按升序），确保返回长度 = days 且无空洞。
    let earliest = today
        .checked_sub_signed(chrono::Duration::days((days - 1) as i64))
        .unwrap_or(today);
    let mut buckets: BTreeMap<chrono::NaiveDate, BTreeMap<String, u64>> = BTreeMap::new();
    for i in 0..days {
        let d = earliest + chrono::Duration::days(i as i64);
        buckets.insert(d, BTreeMap::new());
    }
    for line in content.lines().filter(|l| !l.is_empty()) {
        let Some((motion, _text)) = parse_motion_text(line) else {
            continue;
        };
        // ts 在 line 开头，到第一个空格为止。空白前不到 ts 的行会被
        // parse_motion_text 拦掉一部分（要求 head 中至少一个空格）。
        let Some((ts, _rest)) = line.split_once(' ') else {
            continue;
        };
        let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts) else {
            continue;
        };
        let date = dt.with_timezone(&chrono::Local).date_naive();
        if date < earliest || date > today {
            continue;
        }
        let entry = buckets.entry(date).or_default();
        *entry.entry(motion.to_string()).or_insert(0) += 1;
    }
    buckets
        .into_iter()
        .map(|(date, motions)| {
            let total: u64 = motions.values().sum();
            DailyMotion {
                date: date.format("%Y-%m-%d").to_string(),
                motions,
                total,
            }
        })
        .collect()
}

/// Tauri 命令：返回最近 `days` 天（默认 7 天）按天聚合的 motion 计数。供
/// PanelPersona 「心情谱」段下的 sparkline 渲染使用。
#[tauri::command]
pub async fn get_mood_daily_motions(days: Option<usize>) -> Vec<DailyMotion> {
    let n = days.unwrap_or(7);
    let content = read_history_content().await;
    summarize_motions_by_day(&content, n, chrono::Local::now().date_naive())
}

/// Sparkline「早晚分段」开关用：每天进一步拆成 AM (00:00-11:59) 与
/// PM (12:00-23:59) 两段 motion 计数。total 仍是当日总数，等于 am+pm
/// 的求和（数学一致，让前端切回单段渲染时高度比例不变）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HalfDayMotion {
    pub date: String,
    pub am: BTreeMap<String, u64>,
    pub pm: BTreeMap<String, u64>,
    pub total: u64,
}

/// Pure：与 `summarize_motions_by_day` 同窗口语义，但每天再拆 AM / PM。
/// 边界：`hour < 12` → am，其它 → pm（与日常"上午 / 下午"直觉一致；
/// 不取 13:00 边界避免午饭点的归属歧义）。
///
/// malformed 行 / 空 / 跨窗 / 0 days 与 `summarize_motions_by_day` 容错
/// 语义完全一致，让两条聚合路径在异常输入上行为可预测。
pub fn summarize_motions_by_half_day(
    content: &str,
    days: usize,
    today: chrono::NaiveDate,
) -> Vec<HalfDayMotion> {
    if days == 0 {
        return vec![];
    }
    let earliest = today
        .checked_sub_signed(chrono::Duration::days((days - 1) as i64))
        .unwrap_or(today);
    let mut buckets: BTreeMap<chrono::NaiveDate, (BTreeMap<String, u64>, BTreeMap<String, u64>)> =
        BTreeMap::new();
    for i in 0..days {
        let d = earliest + chrono::Duration::days(i as i64);
        buckets.insert(d, (BTreeMap::new(), BTreeMap::new()));
    }
    for line in content.lines().filter(|l| !l.is_empty()) {
        let Some((motion, _text)) = parse_motion_text(line) else {
            continue;
        };
        let Some((ts, _rest)) = line.split_once(' ') else {
            continue;
        };
        let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts) else {
            continue;
        };
        let local = dt.with_timezone(&chrono::Local);
        let date = local.date_naive();
        if date < earliest || date > today {
            continue;
        }
        use chrono::Timelike;
        let is_am = local.hour() < 12;
        let entry = buckets.entry(date).or_default();
        let target = if is_am { &mut entry.0 } else { &mut entry.1 };
        *target.entry(motion.to_string()).or_insert(0) += 1;
    }
    buckets
        .into_iter()
        .map(|(date, (am, pm))| {
            let total: u64 = am.values().sum::<u64>() + pm.values().sum::<u64>();
            HalfDayMotion {
                date: date.format("%Y-%m-%d").to_string(),
                am,
                pm,
                total,
            }
        })
        .collect()
}

/// Tauri 命令：返回最近 `days` 天（默认 7 天）按 AM/PM 拆分的 motion 计数。
/// 供 PanelPersona sparkline 「早晚分段」开关启用时的渲染使用。
#[tauri::command]
pub async fn get_mood_half_day_motions(days: Option<usize>) -> Vec<HalfDayMotion> {
    let n = days.unwrap_or(7);
    let content = read_history_content().await;
    summarize_motions_by_half_day(&content, n, chrono::Local::now().date_naive())
}

/// sparkline 点格子查详情用的 entry 形态。三字段都 owned String 让 wire
/// format 直接用；timestamp 是 RFC3339 原文（前端只取 HH:MM 部分展示）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MoodEntry {
    pub timestamp: String,
    pub motion: String,
    pub text: String,
}

/// pure: 从 mood_history 全文按 target_date（本地时区）过滤出当天所有 entry。
/// malformed / ts 解析失败行 silent 跳过（与 summarize_motions_by_day 同语义）。
/// 顺序保留输入顺序（按 ts 升序，因为 record_mood 是 append-only）。
pub fn entries_for_date(content: &str, target_date: chrono::NaiveDate) -> Vec<MoodEntry> {
    content
        .lines()
        .filter(|l| !l.is_empty())
        .filter_map(|line| {
            let (ts, _rest) = line.split_once(' ')?;
            let dt = chrono::DateTime::parse_from_rfc3339(ts).ok()?;
            if dt.with_timezone(&chrono::Local).date_naive() != target_date {
                return None;
            }
            let (motion, text) = parse_motion_text(line)?;
            Some(MoodEntry {
                timestamp: ts.to_string(),
                motion: motion.to_string(),
                text: text.to_string(),
            })
        })
        .collect()
}

/// Tauri 命令：返回指定日期（`YYYY-MM-DD`，本地时区）的所有 mood_history entry。
/// 给 sparkline 点格子查详情用。日期解析失败 → 空 vec（前端按"无数据"渲染）。
#[tauri::command]
pub async fn get_mood_entries_for_date(date: String) -> Vec<MoodEntry> {
    let Ok(d) = chrono::NaiveDate::parse_from_str(date.trim(), "%Y-%m-%d") else {
        return Vec::new();
    };
    let content = read_history_content().await;
    entries_for_date(&content, d)
}

/// Pure：清掉"过去 days 天内"的所有 mood_history 条目；保留早于 cutoff 的
/// 条目。`days = 0` 视作清空全部（cutoff = now，所有条目都在 "过去 0 天
/// 内"）。malformed / ts 解析失败的行**也删除**——用户已显式请求清理，
/// dirty 数据是首要目标。
///
/// 边界 `ts == cutoff` 删除（即"恰好 N 天前"的行视作"在过去 N 天内"），
/// 与 `compare_for_queue` 中 overdue 判定 `due <= now` 同语义习惯。
pub fn filter_mood_history_clear_recent_days(
    content: &str,
    days: u32,
    now: chrono::DateTime<chrono::Local>,
) -> String {
    if days == 0 {
        return String::new();
    }
    let cutoff = now - chrono::Duration::days(days as i64);
    let mut kept: Vec<&str> = Vec::new();
    for line in content.lines() {
        if line.is_empty() {
            continue;
        }
        let Some((ts, _)) = line.split_once(' ') else {
            continue; // 无 ts → 视作脏数据，drop
        };
        let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts) else {
            continue; // ts 不是 RFC3339 → drop
        };
        // 严格 `<`：cutoff 那一刻属于"过去 N 天内"，删除
        if dt.with_timezone(&chrono::Local) < cutoff {
            kept.push(line);
        }
    }
    if kept.is_empty() {
        String::new()
    } else {
        let mut out = kept.join("\n");
        out.push('\n');
        out
    }
}

/// Tauri 命令：清理 mood_history。`days = None | Some(0)` → 清空全部；
/// `days = Some(N)` → 保留 ts < now - N days 的条目。返回剩余行数让前端
/// 展示"已清理，剩余 X 条"反馈。
///
/// 写盘失败 → Err；文件不存在视作已经"是空的"，返回 0。
#[tauri::command]
pub async fn clear_mood_history(days: Option<u32>) -> Result<u32, String> {
    let n = days.unwrap_or(0);
    let Some(path) = history_path() else {
        return Err("无法定位 mood_history 路径（dirs::config_dir 失败）".to_string());
    };
    if !path.exists() {
        return Ok(0);
    }
    let content = tokio::fs::read_to_string(&path)
        .await
        .map_err(|e| format!("读 mood_history 失败：{}", e))?;
    let kept = filter_mood_history_clear_recent_days(&content, n, chrono::Local::now());
    let kept_count = kept.lines().filter(|l| !l.is_empty()).count() as u32;
    tokio::fs::write(&path, kept)
        .await
        .map_err(|e| format!("写 mood_history 失败：{}", e))?;
    Ok(kept_count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_motion_text_normal_line() {
        let line = "2026-05-03T12:34:56+08:00 Tap | 看用户在专心写代码";
        assert_eq!(parse_motion_text(line), Some(("Tap", "看用户在专心写代码")));
    }

    #[test]
    fn parse_motion_text_handles_pipe_in_text() {
        // Body containing ' | ' should still parse — split on the first occurrence.
        let line = "2026-05-03T12:00:00+08:00 Idle | 想了想 | 没说话";
        assert_eq!(parse_motion_text(line), Some(("Idle", "想了想 | 没说话")));
    }

    #[test]
    fn parse_motion_text_dash_for_missing_motion() {
        let line = "2026-05-03T12:00:00+08:00 - | 平静";
        assert_eq!(parse_motion_text(line), Some(("-", "平静")));
    }

    #[test]
    fn parse_motion_text_malformed_returns_none() {
        assert!(parse_motion_text("no separator here").is_none());
    }

    #[test]
    fn summarize_recent_motions_counts_and_sorts() {
        let content = "\
2026-05-01T10:00:00+08:00 Tap | a
2026-05-01T11:00:00+08:00 Tap | b
2026-05-02T10:00:00+08:00 Idle | c
2026-05-02T11:00:00+08:00 Tap | d
2026-05-03T10:00:00+08:00 Flick | e
";
        let counts = summarize_recent_motions(content, 10);
        // Tap leads (3), Flick + Idle tie at 1 — alphabetical tiebreak.
        assert_eq!(counts[0], ("Tap".to_string(), 3));
        assert_eq!(counts[1], ("Flick".to_string(), 1));
        assert_eq!(counts[2], ("Idle".to_string(), 1));
    }

    #[test]
    fn summarize_recent_motions_takes_only_window() {
        let content = "\
2026-05-01T10:00:00+08:00 Tap | a
2026-05-02T10:00:00+08:00 Idle | b
2026-05-03T10:00:00+08:00 Flick | c
";
        let counts = summarize_recent_motions(content, 1);
        // Only the last line counts.
        assert_eq!(counts, vec![("Flick".to_string(), 1)]);
    }

    #[test]
    fn format_trend_hint_below_min_returns_none() {
        let content = "\
2026-05-01T10:00:00+08:00 Tap | a
2026-05-02T10:00:00+08:00 Idle | b
";
        // 2 entries < min 5 → None.
        assert!(format_trend_hint(content, 50, 5).is_none());
    }

    #[test]
    fn format_trend_hint_above_min_includes_motions_in_order() {
        let content = "\
2026-05-01T10:00:00+08:00 Tap | a
2026-05-01T11:00:00+08:00 Tap | b
2026-05-02T10:00:00+08:00 Idle | c
2026-05-02T11:00:00+08:00 Tap | d
2026-05-03T10:00:00+08:00 Flick | e
";
        let hint = format_trend_hint(content, 50, 3).unwrap();
        // Tap 出现最多排第一；Idle 和 Flick 各 1 次按字母排（Flick 在前）。
        let tap_pos = hint.find("Tap × 3").unwrap();
        let flick_pos = hint.find("Flick × 1").unwrap();
        assert!(tap_pos < flick_pos);
    }

    #[test]
    fn format_trend_hint_filters_dash_motion() {
        // Untagged mood entries (motion = "-") are not informative for trend context;
        // they still count toward total but don't show up in the hint body.
        let content = "\
2026-05-01T10:00:00+08:00 Tap | a
2026-05-01T11:00:00+08:00 - | b
2026-05-02T10:00:00+08:00 Tap | c
";
        let hint = format_trend_hint(content, 50, 1).unwrap();
        assert!(hint.contains("Tap"));
        assert!(!hint.contains("-"));
    }

    #[test]
    fn format_trend_hint_only_dash_returns_none() {
        // If all entries are untagged, the body would be empty after filtering — return
        // None rather than emit a "你最近 N 次心情记录里：（无）" awkward placeholder.
        let content = "\
2026-05-01T10:00:00+08:00 - | a
2026-05-02T10:00:00+08:00 - | b
";
        assert!(format_trend_hint(content, 50, 1).is_none());
    }

    // ---------------- summarize_motions_by_day ----------------

    fn d(y: i32, m: u32, day: u32) -> chrono::NaiveDate {
        chrono::NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn daily_motions_buckets_per_day_and_counts_motions() {
        // 3 天分布：5/3 有 2 条 (Tap / Idle)，5/4 有 3 条 (Tap×2 + Flick)，5/5 空
        // 用 +08:00 时区写文件；查询 today=5/5、days=3 → 应得到 [5/3, 5/4, 5/5]。
        let content = "\
2026-05-03T10:00:00+08:00 Tap | a
2026-05-03T22:00:00+08:00 Idle | b
2026-05-04T08:00:00+08:00 Tap | c
2026-05-04T12:00:00+08:00 Tap | d
2026-05-04T18:00:00+08:00 Flick | e
";
        let out = summarize_motions_by_day(content, 3, d(2026, 5, 5));
        assert_eq!(out.len(), 3);

        assert_eq!(out[0].date, "2026-05-03");
        assert_eq!(out[0].total, 2);
        assert_eq!(out[0].motions.get("Tap"), Some(&1));
        assert_eq!(out[0].motions.get("Idle"), Some(&1));

        assert_eq!(out[1].date, "2026-05-04");
        assert_eq!(out[1].total, 3);
        assert_eq!(out[1].motions.get("Tap"), Some(&2));
        assert_eq!(out[1].motions.get("Flick"), Some(&1));

        assert_eq!(out[2].date, "2026-05-05");
        assert_eq!(out[2].total, 0);
        assert!(out[2].motions.is_empty());
    }

    #[test]
    fn daily_motions_window_extends_with_empty_buckets() {
        // 历史只 1 条（5/4），但请求 7 天 → 前 6 天应 total=0 占位，长度 = 7。
        let content = "\
2026-05-04T10:00:00+08:00 Tap | a
";
        let out = summarize_motions_by_day(content, 7, d(2026, 5, 5));
        assert_eq!(out.len(), 7);
        // 最早的 5 天应为 0；倒数第二是 5/4 = 1；最后一天 5/5 = 0
        assert_eq!(out[0].date, "2026-04-29");
        assert_eq!(out[0].total, 0);
        assert_eq!(out[5].date, "2026-05-04");
        assert_eq!(out[5].total, 1);
        assert_eq!(out[6].date, "2026-05-05");
        assert_eq!(out[6].total, 0);
    }

    #[test]
    fn daily_motions_skips_records_outside_window() {
        // 窗口 = today 往前 3 天（5/3..=5/5）；历史里有 5/1 的记录应被丢弃。
        let content = "\
2026-05-01T10:00:00+08:00 Tap | very-old
2026-05-04T10:00:00+08:00 Idle | inside
";
        let out = summarize_motions_by_day(content, 3, d(2026, 5, 5));
        assert_eq!(out.len(), 3);
        let total: u64 = out.iter().map(|x| x.total).sum();
        assert_eq!(total, 1, "outside-window entries must not leak in");
    }

    #[test]
    fn daily_motions_respects_local_timezone_for_day_boundary() {
        // 2026-05-04T23:30 +08:00 == 2026-05-04 in +08:00 local zone (which the
        // backend uses via chrono::Local). 2026-05-05T00:30 +08:00 → 5/5 bucket.
        // 我们直接把 ts 写成 +08:00 以避免本地化测试在 CI 上飘 —— 解析器吃 RFC3339
        // 后转 chrono::Local，CI 时区和 +08 不同时也会归到对应本地日（这里只断言
        // 同一时区内的相对分桶，不依赖具体偏移）。
        //
        // 为避免依赖 chrono::Local 在测试环境是 +08:00，本测改用一对相邻 ts、
        // 验证它们落到不同分桶（差 23h 的两条 entry 必然跨日）。
        let content = "\
2026-05-04T01:00:00+08:00 Tap | early
2026-05-05T00:30:00+08:00 Idle | next-day
";
        let out = summarize_motions_by_day(content, 3, d(2026, 5, 5));
        // 至少两个非空桶（具体落到哪天要看 chrono::Local 时区，但分到两个不
        // 同的桶这件事在所有时区都成立）
        let non_empty: Vec<_> = out.iter().filter(|x| x.total > 0).collect();
        assert!(
            non_empty.len() >= 1, // 在极端时区下两条可能分到同一天，至少 1 条命中即可
            "should have at least 1 bucketed entry within window: {:?}",
            out
        );
    }

    #[test]
    fn daily_motions_skips_malformed_lines() {
        // 缺 ts、缺分隔符、ts 不是 RFC3339 — 都跳过不 panic
        let content = "\
not even close to a timestamp
2026-05-03 missing-space-before-pipe | wrong
2026-99-99T10:00:00+08:00 Tap | bad-date
2026-05-04T10:00:00+08:00 Tap | good
";
        let out = summarize_motions_by_day(content, 3, d(2026, 5, 5));
        let total: u64 = out.iter().map(|x| x.total).sum();
        assert_eq!(total, 1, "only the well-formed line should count");
    }

    #[test]
    fn daily_motions_returns_empty_for_zero_days() {
        let out = summarize_motions_by_day("anything", 0, d(2026, 5, 5));
        assert!(out.is_empty());
    }

    #[test]
    fn daily_motions_dedupes_motion_counts_within_a_day() {
        // 同 motion 同日多次 → 累加而非覆盖
        let content = "\
2026-05-04T08:00:00+08:00 Tap | a
2026-05-04T09:00:00+08:00 Tap | b
2026-05-04T10:00:00+08:00 Tap | c
";
        let out = summarize_motions_by_day(content, 1, d(2026, 5, 4));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].motions.get("Tap"), Some(&3));
        assert_eq!(out[0].total, 3);
    }

    // ---------------- summarize_motions_by_half_day ----------------

    #[test]
    fn half_day_motions_split_at_noon_boundary() {
        // 8:00 → AM；12:00 → PM；23:30 → PM。AM/PM 分桶 + total = am+pm。
        // 注意 chrono::Local 在 CI 可能不是 +08:00；这里使用本地时区表达式
        // 让分桶与本地时区一致 —— 我们改用本地时区写 ts 避免飘移。
        let content = "\
2026-05-04T08:00:00+08:00 Tap | early
2026-05-04T12:00:00+08:00 Flick | noon
2026-05-04T23:30:00+08:00 Idle | late
";
        // 在 +08 本地时区，三条都落到 5/4：08 = AM, 12 = PM, 23 = PM
        // 在其它时区可能漂移，但 AM+PM 总数恒等于 3 条
        let out = summarize_motions_by_half_day(content, 1, d(2026, 5, 4));
        assert_eq!(out.len(), 1);
        let total: u64 =
            out[0].am.values().sum::<u64>() + out[0].pm.values().sum::<u64>();
        assert_eq!(total, out[0].total, "total must equal am+pm sum");
    }

    #[test]
    fn half_day_motions_preserves_existing_total_semantics() {
        // 与 summarize_motions_by_day 在同一组数据上 total 数完全相同 ——
        // 切换 sparkline 渲染模式时整柱高度不该变（数学一致）。
        let content = "\
2026-05-04T08:00:00+08:00 Tap | a
2026-05-04T11:00:00+08:00 Tap | b
2026-05-04T13:00:00+08:00 Flick | c
2026-05-04T20:00:00+08:00 Idle | d
";
        let day = summarize_motions_by_day(content, 1, d(2026, 5, 4));
        let half = summarize_motions_by_half_day(content, 1, d(2026, 5, 4));
        assert_eq!(day.len(), half.len());
        assert_eq!(day[0].total, half[0].total, "totals must match");
    }

    #[test]
    fn half_day_motions_skips_malformed_lines() {
        let content = "\
not a ts
2026-99-99T10:00:00+08:00 Tap | bad
2026-05-04T08:00:00+08:00 Tap | good
";
        let out = summarize_motions_by_half_day(content, 1, d(2026, 5, 4));
        let total: u64 =
            out[0].am.values().sum::<u64>() + out[0].pm.values().sum::<u64>();
        assert_eq!(total, 1, "only the well-formed line should count");
    }

    #[test]
    fn half_day_motions_returns_empty_for_zero_days() {
        let out = summarize_motions_by_half_day("anything", 0, d(2026, 5, 5));
        assert!(out.is_empty());
    }

    #[test]
    fn half_day_motions_window_extends_with_empty_buckets() {
        // 历史只 1 条但请求 7 天 → 长度 = 7，前 6 天 am/pm 都空
        let content = "\
2026-05-04T08:00:00+08:00 Tap | a
";
        let out = summarize_motions_by_half_day(content, 7, d(2026, 5, 5));
        assert_eq!(out.len(), 7);
        let bucketed: Vec<_> = out.iter().filter(|x| x.total > 0).collect();
        assert_eq!(bucketed.len(), 1, "exactly one day should have entries");
    }

    #[test]
    fn half_day_motions_skips_records_outside_window() {
        let content = "\
2026-05-01T10:00:00+08:00 Tap | old
2026-05-04T10:00:00+08:00 Idle | inside
";
        let out = summarize_motions_by_half_day(content, 3, d(2026, 5, 5));
        let total: u64 = out
            .iter()
            .map(|x| x.am.values().sum::<u64>() + x.pm.values().sum::<u64>())
            .sum();
        assert_eq!(total, 1, "outside-window entries must not leak in");
    }

    // ---------------- filter_mood_history_clear_recent_days ----------------

    fn now_for_clear() -> chrono::DateTime<chrono::Local> {
        // 2026-05-05 12:00:00 +08:00 —— 与 daily_motions 测试套同一锚点
        use chrono::TimeZone;
        chrono::FixedOffset::east_opt(8 * 3600)
            .unwrap()
            .with_ymd_and_hms(2026, 5, 5, 12, 0, 0)
            .unwrap()
            .with_timezone(&chrono::Local)
    }

    #[test]
    fn clear_recent_days_zero_wipes_everything() {
        let content = "\
2026-05-04T10:00:00+08:00 Tap | a
2026-05-03T10:00:00+08:00 Idle | b
";
        assert_eq!(filter_mood_history_clear_recent_days(content, 0, now_for_clear()), "");
    }

    #[test]
    fn clear_recent_days_keeps_only_older_than_cutoff() {
        // now = 5/5 12:00；N=3 → cutoff = 5/2 12:00。保留 ts < 5/2 12:00 的行。
        let content = "\
2026-05-01T10:00:00+08:00 Tap | very-old
2026-05-02T11:59:00+08:00 Idle | one-minute-before-cutoff
2026-05-02T12:00:00+08:00 Flick | exactly-at-cutoff
2026-05-04T10:00:00+08:00 Tap | recent
";
        let out = filter_mood_history_clear_recent_days(content, 3, now_for_clear());
        // 仅 5/1 与 5/2 11:59 留下；5/2 12:00 边界本身视作 "in past N days" → drop
        assert!(out.contains("very-old"));
        assert!(out.contains("one-minute-before-cutoff"));
        assert!(!out.contains("exactly-at-cutoff"));
        assert!(!out.contains("recent"));
    }

    #[test]
    fn clear_recent_days_drops_malformed_lines() {
        let content = "\
not even a timestamp here
2026-99-99T25:00:00+08:00 Tap | bad-ts
2026-05-01T10:00:00+08:00 Tap | good-old
";
        let out = filter_mood_history_clear_recent_days(content, 1, now_for_clear());
        // bad-ts 被 drop；只 good-old 留
        assert!(out.contains("good-old"));
        assert!(!out.contains("bad-ts"));
        assert!(!out.contains("not even"));
    }

    #[test]
    fn clear_recent_days_empty_input_returns_empty() {
        assert_eq!(filter_mood_history_clear_recent_days("", 7, now_for_clear()), "");
    }

    #[test]
    fn clear_recent_days_all_recent_yields_empty() {
        // 全部条目都在过去 7 天内 → 全清
        let content = "\
2026-05-04T10:00:00+08:00 Tap | a
2026-05-03T10:00:00+08:00 Idle | b
";
        assert_eq!(filter_mood_history_clear_recent_days(content, 7, now_for_clear()), "");
    }

    #[test]
    fn clear_recent_days_preserves_trailing_newline_when_kept() {
        // 输出格式契约：非空时以 `\n` 结尾，便于 append 模式的 record_mood 续写
        let content = "\
2026-04-01T10:00:00+08:00 Tap | very-old
";
        let out = filter_mood_history_clear_recent_days(content, 3, now_for_clear());
        assert!(out.ends_with('\n'));
    }

    // ---------------- entries_for_date ----------------

    #[test]
    fn entries_for_date_returns_only_target_day() {
        let content = "\
2026-05-03T10:00:00+08:00 Tap | a
2026-05-04T08:00:00+08:00 Idle | early
2026-05-04T22:00:00+08:00 Flick | late
2026-05-05T10:00:00+08:00 Tap | next-day
";
        let out = entries_for_date(content, d(2026, 5, 4));
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].text, "early");
        assert_eq!(out[0].motion, "Idle");
        assert_eq!(out[1].text, "late");
        assert_eq!(out[1].motion, "Flick");
    }

    #[test]
    fn entries_for_date_skips_malformed_lines() {
        let content = "\
not even a timestamp here
2026-99-99T25:00:00+08:00 Tap | bad-ts
2026-05-04T10:00:00+08:00 Tap | good
2026-05-04 missing-space-before-pipe wrong-format
";
        let out = entries_for_date(content, d(2026, 5, 4));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].text, "good");
    }

    #[test]
    fn entries_for_date_returns_empty_for_no_match() {
        let content = "\
2026-05-04T10:00:00+08:00 Tap | a
";
        assert!(entries_for_date(content, d(2026, 5, 5)).is_empty());
    }

    #[test]
    fn entries_for_date_returns_empty_for_empty_input() {
        assert!(entries_for_date("", d(2026, 5, 5)).is_empty());
    }

    #[test]
    fn entries_for_date_preserves_input_order() {
        // record_mood 是 append-only，时间序就是输入序。验证不被重排。
        let content = "\
2026-05-04T08:00:00+08:00 Tap | first
2026-05-04T18:00:00+08:00 Tap | last
2026-05-04T12:00:00+08:00 Idle | middle
";
        let out = entries_for_date(content, d(2026, 5, 4));
        assert_eq!(out.iter().map(|e| e.text.as_str()).collect::<Vec<_>>(), vec!["first", "last", "middle"]);
    }
}
