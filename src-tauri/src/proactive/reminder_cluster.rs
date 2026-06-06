//! Recurring reminder 探测（GOAL 004）：从 butler_history.log 的
//! `reminder` 事件流中找「最近 N 天反复出现的同形 reminder」cluster，
//! 给 reminder hint 注入「要不要改成每日自动起」的提议。
//!
//! 与 [`memory_follow_up`] / [`welcome_back`] 同结构：纯函数 + const
//! 阈值 + 单测覆盖核心边界。落盘 / LLM 调用都在 `proactive.rs` 那层。
//!
//! 触发链路（与 reminders.rs 协作）：
//! 1. `proactive::build_reminders_hint` 检测到 due-now reminder 时调
//!    `record_reminder_fire` (per-day dedup)，把事件写入 butler_history。
//! 2. 同一函数继续调 [`detect_clusters`] 扫历史，按 (normalized_topic,
//!    HH:MM bucket) 聚类。
//! 3. ≥ [`MIN_HITS_FOR_PROPOSAL`] 的 cluster → [`format_proposal_hint`]
//!    生成提议文案附在 reminder hint 尾巴；LLM 看到后自然开口邀请用户
//!    切换为 daily 模式。
//! 4. 若用户同意，LLM 用 `memory_edit` 把 reminder description 换成
//!    `[recur-daily: HH:MM] topic` 前缀（parse 在 reminders.rs 已支持）。

use chrono::{DateTime, Duration, Local, NaiveDate};

/// 聚类回看窗口（天）。GOAL 写「近 14d」。
pub const CLUSTER_WINDOW_DAYS: i64 = 14;

/// 单 cluster 触发「要不要 recurring」提议所需的命中天数下限。GOAL「N≥3」。
pub const MIN_HITS_FOR_PROPOSAL: usize = 3;

/// HH:MM bucket 容差（分钟）。两次触发若 HH:MM 相距 ≤ 此值就视作同一时
/// 段聚类。30 分钟覆盖「我每天 8 点 / 8:15 / 8:30 各设过一次」自然模糊
/// 性；更小会让用户日常作息漂移把同一行为打散到多 cluster。
pub const TIME_BUCKET_TOLERANCE_MINUTES: i64 = 30;

/// 文本相似度阈值。简单 Jaccard 字符集相似度即可：sub-string 重写比例 ≥
/// 此值算同 topic。0.65 经验值 ——「吃药 / 该吃药 / 吃个药」都能聚到一起，
/// 而「吃药」「健身」聚不到一起。
pub const TOPIC_SIMILARITY_THRESHOLD: f32 = 0.65;

/// 单个 cluster 的可观察值。`hits` 是窗口内命中的去重日期数（同日多触
/// 发只算 1，避免一日内 reminder 反复被 due-now 检测放大计数）。
#[derive(Debug, Clone)]
pub struct Cluster {
    /// 代表性 topic。挑最近一次命中的 topic（用户口语会演化，最新的最像
    /// 当前心智模型）。
    pub topic: String,
    /// 代表性时间点（H, M）。最近一次命中的 HH:MM —— 提议落盘时直接用。
    pub hour: u8,
    pub minute: u8,
    /// 窗口内去重「日期 × 同 cluster」的命中数。
    pub hits: usize,
    /// 窗口内见过的 reminder 标题集合（同 topic 不同 title 是常见场景：
    /// 用户每天手动创建条目，title 略有不同）。给 LLM 提议时一并列出。
    pub titles: Vec<String>,
}

/// 单条历史事件结构。`parse_reminder_events_from_history` 拍扁
/// butler_history.log 文本得到的清洁 view。
#[derive(Debug, Clone)]
pub struct ReminderEvent {
    pub date: NaiveDate,
    pub hour: u8,
    pub minute: u8,
    pub title: String,
    pub topic: String,
}

/// Pure：从 butler_history.log 全文中过滤出 `reminder` action 的事件
/// 流。snippet 段约定为 `<HH:MM> <topic>`（由 `format_reminder_log_body`
/// 拼出），便于回解 hour / minute / topic。无效行 silently 丢弃。
pub fn parse_reminder_events_from_history(content: &str) -> Vec<ReminderEvent> {
    let mut out = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (ts_str, action, title, snippet) =
            match crate::butler_history::parse_butler_history_line(line) {
                Some(t) => t,
                None => continue,
            };
        if action != "reminder" {
            continue;
        }
        let dt = match DateTime::parse_from_rfc3339(ts_str) {
            Ok(t) => t.with_timezone(&Local),
            Err(_) => continue,
        };
        let snippet = snippet.trim();
        // snippet = "HH:MM topic" —— 拆 head 是时间，剩下是 topic。容
        // 错：缺时间 token 时 fallback hour/minute 用 ts 推断。
        let (hh, mm, topic) = match parse_log_snippet(snippet) {
            Some(t) => t,
            None => continue,
        };
        out.push(ReminderEvent {
            date: dt.date_naive(),
            hour: hh,
            minute: mm,
            title: title.to_string(),
            topic,
        });
    }
    out
}

fn parse_log_snippet(snippet: &str) -> Option<(u8, u8, String)> {
    let (head, rest) = snippet.split_once(' ')?;
    let (hh_s, mm_s) = head.split_once(':')?;
    let hh: u8 = hh_s.trim().parse().ok()?;
    let mm: u8 = mm_s.trim().parse().ok()?;
    if hh > 23 || mm > 59 {
        return None;
    }
    Some((hh, mm, rest.trim().to_string()))
}

/// Pure formatter：把一次 reminder 触发的 snippet 拼好供 record_event 用。
/// 单一 fmt 函数让记录方与读取方对协议达成一致，避免格式漂移。
pub fn format_reminder_log_body(hour: u8, minute: u8, topic: &str) -> String {
    format!("{:02}:{:02} {}", hour, minute, topic.trim())
}

/// Jaccard 字符集相似度。零依赖 / 中英文都不需 tokenize / 对短文本
/// （reminder topic 通常 ≤ 10 字）准确度足够。空字符串相似度 0。
fn jaccard_char(a: &str, b: &str) -> f32 {
    use std::collections::HashSet;
    let aset: HashSet<char> = a.chars().filter(|c| !c.is_whitespace()).collect();
    let bset: HashSet<char> = b.chars().filter(|c| !c.is_whitespace()).collect();
    if aset.is_empty() || bset.is_empty() {
        return 0.0;
    }
    let inter = aset.intersection(&bset).count() as f32;
    let union = aset.union(&bset).count() as f32;
    inter / union
}

/// Pure：把 events 按 (相似 topic, ±N 分钟时段) 聚类，返回命中 ≥
/// [`MIN_HITS_FOR_PROPOSAL`] 的 clusters。命中按「不同日期」去重 ——
/// 一日内同 reminder 多次 due-now 检测只算 1 天。`now` 用于窗口边界
/// （`CLUSTER_WINDOW_DAYS` 内的事件才入聚类池）。
///
/// 算法：贪心 —— 按时间升序遍历，每个事件尝试合并到现有 cluster；不
/// 命中则新建 cluster。复杂度 O(events × clusters)，桌面宠物量级（每
/// 天 ≤ 几条 reminder × 14 天）够用。
pub fn detect_clusters(events: &[ReminderEvent], now: DateTime<Local>) -> Vec<Cluster> {
    let cutoff = (now - Duration::days(CLUSTER_WINDOW_DAYS)).date_naive();
    let mut clusters: Vec<ClusterAccum> = Vec::new();
    let mut sorted: Vec<&ReminderEvent> =
        events.iter().filter(|e| e.date >= cutoff).collect();
    sorted.sort_by_key(|e| e.date);

    for e in sorted {
        let merged = clusters
            .iter_mut()
            .find(|c| can_merge(c, e))
            .map(|c| {
                c.dates.insert(e.date);
                if !c.titles.contains(&e.title) {
                    c.titles.push(e.title.clone());
                }
                c.latest_topic = e.topic.clone();
                c.latest_hour = e.hour;
                c.latest_minute = e.minute;
            });
        if merged.is_none() {
            let mut dates = std::collections::HashSet::new();
            dates.insert(e.date);
            clusters.push(ClusterAccum {
                latest_topic: e.topic.clone(),
                latest_hour: e.hour,
                latest_minute: e.minute,
                titles: vec![e.title.clone()],
                dates,
            });
        }
    }

    clusters
        .into_iter()
        .filter_map(|c| {
            if c.dates.len() >= MIN_HITS_FOR_PROPOSAL {
                Some(Cluster {
                    topic: c.latest_topic,
                    hour: c.latest_hour,
                    minute: c.latest_minute,
                    hits: c.dates.len(),
                    titles: c.titles,
                })
            } else {
                None
            }
        })
        .collect()
}

struct ClusterAccum {
    latest_topic: String,
    latest_hour: u8,
    latest_minute: u8,
    titles: Vec<String>,
    dates: std::collections::HashSet<NaiveDate>,
}

fn can_merge(c: &ClusterAccum, e: &ReminderEvent) -> bool {
    // 时段差：分钟绝对差 ≤ TOLERANCE
    let c_total = c.latest_hour as i64 * 60 + c.latest_minute as i64;
    let e_total = e.hour as i64 * 60 + e.minute as i64;
    if (c_total - e_total).abs() > TIME_BUCKET_TOLERANCE_MINUTES {
        return false;
    }
    // 文本相似度
    jaccard_char(&c.latest_topic, &e.topic) >= TOPIC_SIMILARITY_THRESHOLD
}

/// Pure：把 cluster 信息拼成一段 LLM 看的提议文案。多 cluster 时按 hits
/// 倒序 + 最多 2 条避免占太多上下文。空 clusters → 空串。
///
/// 文案显式给 LLM 「同意 → 用 memory_edit 改前缀为 recur-daily」的协议
/// 提示，让用户的口头同意经 LLM 转成具体动作（避开本轮做 NL accept-parse
/// 的复杂度）。
pub fn format_proposal_hint(clusters: &[Cluster]) -> String {
    if clusters.is_empty() {
        return String::new();
    }
    let mut sorted: Vec<&Cluster> = clusters.iter().collect();
    sorted.sort_by(|a, b| b.hits.cmp(&a.hits));
    let mut lines = vec![
        "【周期性观察】系统注意到近期反复出现的提醒模式（仅信号；你判断是否值得开口）：".to_string(),
    ];
    for c in sorted.iter().take(2) {
        let titles_label = if c.titles.is_empty() {
            "(无相关 todo)".to_string()
        } else {
            c.titles.join(" / ")
        };
        lines.push(format!(
            "· 「{}」过去 {} 天命中 {} 次（约 {:02}:{:02}）·相关 todo: {}",
            c.topic, CLUSTER_WINDOW_DAYS, c.hits, c.hour, c.minute, titles_label
        ));
    }
    lines.push(
        "如果你判断时机合适，可以顺口问主人是否愿意改成每日自动起；\
         主人同意时用 `memory_edit` 在 `todo` category 把对应条目 description 改成 \
         `[recur-daily: HH:MM] <topic>` 前缀（解析层会按每天自动重新到点）；\
         不愿意就 acknowledge 一下不强推。"
            .to_string(),
    );
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(date_offset_days: i64, hh: u8, mm: u8, title: &str, topic: &str) -> ReminderEvent {
        let d = (Local::now() - Duration::days(date_offset_days)).date_naive();
        ReminderEvent {
            date: d,
            hour: hh,
            minute: mm,
            title: title.to_string(),
            topic: topic.to_string(),
        }
    }

    #[test]
    fn detect_skips_clusters_below_threshold() {
        let evs = vec![
            event(1, 8, 0, "今日吃药", "吃药"),
            event(2, 8, 0, "今日吃药", "吃药"),
        ];
        // 只 2 天 → 低于 MIN_HITS_FOR_PROPOSAL=3 → 没 cluster
        assert!(detect_clusters(&evs, Local::now()).is_empty());
    }

    #[test]
    fn detect_clusters_when_threshold_met() {
        let evs = vec![
            event(1, 8, 0, "吃药 a", "吃药"),
            event(2, 8, 15, "吃药 b", "该吃药"), // 15min 内、相似度高
            event(3, 8, 0, "吃药 c", "吃药"),
        ];
        let clusters = detect_clusters(&evs, Local::now());
        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].hits, 3);
        assert!(clusters[0].topic.contains("吃药"));
    }

    #[test]
    fn detect_treats_same_day_repeat_as_single_hit() {
        // 同一天 2 条都因 due-now 反复触发 record_event：本算法按
        // dates HashSet 去重 → 1 天命中 1 次。
        let evs = vec![
            event(1, 8, 0, "吃药", "吃药"),
            event(1, 8, 0, "吃药", "吃药"),
            event(1, 8, 0, "吃药", "吃药"),
        ];
        // hits = 1 (一天) < 3 → 不该生成 cluster
        assert!(detect_clusters(&evs, Local::now()).is_empty());
    }

    #[test]
    fn detect_drops_events_outside_window() {
        // 20d 前 + 3 天近 → 老的丢，新的不够 3 天 → 无 cluster
        let evs = vec![
            event(20, 8, 0, "吃药", "吃药"),
            event(21, 8, 0, "吃药", "吃药"),
            event(22, 8, 0, "吃药", "吃药"),
            event(1, 8, 0, "吃药", "吃药"),
            event(2, 8, 0, "吃药", "吃药"),
        ];
        let clusters = detect_clusters(&evs, Local::now());
        // 20-22d 前都在 14d 外被丢；只剩 2 个最近的 < 3
        assert!(clusters.is_empty(), "got {:?}", clusters);
    }

    #[test]
    fn detect_does_not_merge_distant_topics() {
        // 「吃药」与「健身」字符集几乎无重叠 → 不该聚到一起
        let evs = vec![
            event(1, 8, 0, "吃药", "吃药"),
            event(2, 8, 0, "吃药", "吃药"),
            event(3, 8, 0, "健身打卡", "健身"),
        ];
        let clusters = detect_clusters(&evs, Local::now());
        // 吃药 2 天（< 3）+ 健身 1 天（< 3）→ 都不出 cluster
        assert!(clusters.is_empty());
    }

    #[test]
    fn detect_does_not_merge_distant_times() {
        // 同 topic 但时段差 2h → 视作不同 cluster
        let evs = vec![
            event(1, 8, 0, "吃药 a", "吃药"),
            event(2, 8, 0, "吃药 a", "吃药"),
            event(3, 8, 0, "吃药 a", "吃药"),
            event(1, 22, 0, "吃药 n", "吃药"),
            event(2, 22, 0, "吃药 n", "吃药"),
            event(3, 22, 0, "吃药 n", "吃药"),
        ];
        let clusters = detect_clusters(&evs, Local::now());
        assert_eq!(clusters.len(), 2);
    }

    #[test]
    fn parse_snippet_round_trip() {
        let body = format_reminder_log_body(8, 30, "吃药");
        let (h, m, t) = parse_log_snippet(&body).unwrap();
        assert_eq!(h, 8);
        assert_eq!(m, 30);
        assert_eq!(t, "吃药");
    }

    #[test]
    fn jaccard_close_topics() {
        assert!(jaccard_char("吃药", "该吃药") >= TOPIC_SIMILARITY_THRESHOLD);
        assert!(jaccard_char("吃药", "健身") < TOPIC_SIMILARITY_THRESHOLD);
    }

    #[test]
    fn format_proposal_returns_empty_for_no_clusters() {
        assert_eq!(format_proposal_hint(&[]), "");
    }

    #[test]
    fn format_proposal_caps_at_two_clusters() {
        let clusters: Vec<Cluster> = (0..5)
            .map(|i| Cluster {
                topic: format!("t{}", i),
                hour: 8,
                minute: 0,
                hits: 3 + i,
                titles: vec![format!("title-{}", i)],
            })
            .collect();
        let s = format_proposal_hint(&clusters);
        // 最高 hits 优先：t4 / t3 出现，t0 / t1 不该出现
        assert!(s.contains("「t4」"));
        assert!(s.contains("「t3」"));
        assert!(!s.contains("「t0」"));
    }
}
