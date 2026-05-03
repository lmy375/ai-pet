//! Per-utterance user-feedback log (Iter R1).
//!
//! Each line records whether the user replied to or ignored a previous
//! proactive utterance. The classification is computed at the *start* of the
//! next proactive turn by reading the InteractionClock's raw awaiting flag —
//! see `proactive::run_proactive_turn` for the integration point.
//!
//! The log feeds back into the next proactive prompt via `format_feedback_hint`,
//! so the LLM can see "上次你说『...』，用户没回应" / "用户回复了" and adjust
//! tone, cadence, or content. Distinct from `speech_history.log` which only
//! tracks what the pet *said* — this layer captures how the user *received*
//! it.
//!
//! Format per line:
//!   `{ISO timestamp} {kind} | {speech excerpt up to FEEDBACK_EXCERPT_CHARS}`
//!
//! Kinds:
//!   - `replied` — user sent a message between the previous proactive turn
//!     and this one.
//!   - `ignored` — no user message arrived; the bubble auto-dismissed and the
//!     awaiting flag was still set when the next proactive started.

use std::path::PathBuf;

use serde::Serialize;

/// Cap on lines retained in the log. About 30 days at the typical proactive
/// cadence; older entries roll off so the file stays bounded without a
/// background pruner.
pub const FEEDBACK_HISTORY_CAP: usize = 200;

/// How many characters of the previous proactive utterance to keep in each
/// log line. Long enough to distinguish utterances, short enough to keep
/// `recent_feedback` cheap.
pub const FEEDBACK_EXCERPT_CHARS: usize = 40;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FeedbackKind {
    Replied,
    Ignored,
    /// Iter R1b: user clicked the desktop bubble within 5s of it appearing —
    /// active rejection (saw + dismissed), distinct from passive `Ignored`
    /// (no interaction at all). Counted as negative in `negative_signal_ratio`
    /// alongside `Ignored`.
    Dismissed,
}

impl FeedbackKind {
    pub fn as_str(self) -> &'static str {
        match self {
            FeedbackKind::Replied => "replied",
            FeedbackKind::Ignored => "ignored",
            FeedbackKind::Dismissed => "dismissed",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FeedbackEntry {
    /// ISO-formatted local timestamp. Iter R6 surfaces this in the panel
    /// feedback timeline (each entry shows "HH:MM kind | excerpt").
    pub timestamp: String,
    pub kind: FeedbackKind,
    pub excerpt: String,
}

fn history_path() -> Option<PathBuf> {
    crate::commands::debug::log_dir().into()
}

fn file_path() -> Option<PathBuf> {
    history_path().map(|d| d.join("feedback_history.log"))
}

/// Render an entry as a single log line. Pure / testable.
pub fn format_line(timestamp: &str, kind: FeedbackKind, excerpt: &str) -> String {
    let flat = excerpt.replace(['\n', '\r'], " ");
    let truncated: String = if flat.chars().count() <= FEEDBACK_EXCERPT_CHARS {
        flat
    } else {
        let head: String = flat.chars().take(FEEDBACK_EXCERPT_CHARS).collect();
        format!("{}…", head)
    };
    format!("{} {} | {}", timestamp, kind.as_str(), truncated)
}

/// Parse one previously-written line back to a struct. Returns None for any
/// malformed line so the panel and prompt builders can skip silently rather
/// than crash on log corruption.
pub fn parse_line(line: &str) -> Option<FeedbackEntry> {
    // Format: `{ISO ts} {kind} | {excerpt}`
    let (head, excerpt) = line.split_once(" | ")?;
    let mut parts = head.rsplitn(2, ' ');
    let kind_str = parts.next()?;
    let timestamp = parts.next()?.to_string();
    let kind = match kind_str {
        "replied" => FeedbackKind::Replied,
        "ignored" => FeedbackKind::Ignored,
        "dismissed" => FeedbackKind::Dismissed,
        _ => return None,
    };
    Some(FeedbackEntry {
        timestamp,
        kind,
        excerpt: excerpt.to_string(),
    })
}

/// Append a feedback record. Errors are silently swallowed (consistent with
/// the rest of the history-log module pattern in this project) — feedback is
/// observability, not authoritative state.
pub async fn record_event(kind: FeedbackKind, prev_speech_excerpt: &str) {
    let Some(path) = file_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = tokio::fs::create_dir_all(parent).await;
    }
    let timestamp = chrono::Local::now()
        .format("%Y-%m-%dT%H:%M:%S%:z")
        .to_string();
    let line = format_line(&timestamp, kind, prev_speech_excerpt);
    let existing = tokio::fs::read_to_string(&path).await.unwrap_or_default();
    let mut entries: Vec<String> = existing
        .lines()
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect();
    entries.push(line);
    if entries.len() > FEEDBACK_HISTORY_CAP {
        let drop = entries.len() - FEEDBACK_HISTORY_CAP;
        entries.drain(0..drop);
    }
    let _ = tokio::fs::write(&path, format!("{}\n", entries.join("\n"))).await;
}

/// Read the most recent N entries, newest last. Returns empty when the file
/// doesn't exist or is unreadable.
pub async fn recent_feedback(n: usize) -> Vec<FeedbackEntry> {
    let Some(path) = file_path() else {
        return Vec::new();
    };
    let content = tokio::fs::read_to_string(&path).await.unwrap_or_default();
    let mut entries: Vec<FeedbackEntry> = content
        .lines()
        .filter(|l| !l.is_empty())
        .filter_map(parse_line)
        .collect();
    if entries.len() > n {
        let drop = entries.len() - n;
        entries.drain(0..drop);
    }
    entries
}

/// Iter R6: Tauri command for the panel. Returns recent feedback entries
/// newest-first so the panel can render a fresh-on-top timeline. 20 is the
/// fixed window — long enough to spot a "三连忽略" pattern, short enough
/// to render compactly in a collapsible card.
#[tauri::command]
pub async fn get_recent_feedback() -> Vec<FeedbackEntry> {
    let mut entries = recent_feedback(20).await;
    entries.reverse(); // recent_feedback returns oldest-first; panel wants newest-first.
    entries
}

/// Iter R1b: frontend ChatBubble fires this when the user actively dismisses
/// the bubble within the "quick" window (< 5s). Records a `Dismissed` entry
/// against the proactive utterance excerpt. Frontend is the gate-keeper —
/// after the threshold, the click still hides the bubble but does NOT call
/// this command, so passive late-hides don't pollute feedback history.
#[tauri::command]
pub async fn record_bubble_dismissed(excerpt: String) {
    record_event(FeedbackKind::Dismissed, &excerpt).await;
}

/// Iter R7 / R1b: compute the share of *negative-signal* outcomes in
/// `entries`. Negative = `Ignored` OR `Dismissed`. Returns
/// `Some((ratio, total))` where ratio is in 0.0..=1.0 and total is the
/// sample count. Returns `None` for empty input so callers can gate on
/// "have enough data" without an extra check.
///
/// `Dismissed` was added in R1b — counted alongside `Ignored` because
/// both signal "the user did not engage with this turn". Active dismissal
/// is arguably *stronger* negative than passive ignore, but counting them
/// uniformly keeps the adapter step-function (R7's three-band cooldown)
/// auditable: a panel reader can see the dismiss and ignore counts and
/// know they collapse to one ratio.
pub fn negative_signal_ratio(entries: &[FeedbackEntry]) -> Option<(f64, usize)> {
    if entries.is_empty() {
        return None;
    }
    let negative = entries
        .iter()
        .filter(|e| matches!(e.kind, FeedbackKind::Ignored | FeedbackKind::Dismissed))
        .count();
    Some((negative as f64 / entries.len() as f64, entries.len()))
}

/// Iter R7: minimum samples before the adapter has any effect. Below this
/// the base cooldown is returned unchanged — adapting on 1-2 entries would
/// thrash on noise.
pub const FEEDBACK_ADAPT_MIN_SAMPLES: usize = 5;
/// High-ignore band: above this ratio the pet has clearly been overstaying
/// its welcome — multiplier `ADAPT_HIGH_IGNORE_MULTIPLIER` lengthens the
/// cooldown. 0.6 = "more than 60% of recent proactives went unanswered".
pub const ADAPT_HIGH_IGNORE_THRESHOLD: f64 = 0.6;
pub const ADAPT_HIGH_IGNORE_MULTIPLIER: f64 = 2.0;
/// Low-ignore band: below this ratio the user is engaging well — the pet
/// can speak more freely. 0.2 = "fewer than 20% of recent proactives went
/// unanswered". Multiplier shrinks cooldown gently (0.7×, not aggressive).
pub const ADAPT_LOW_IGNORE_THRESHOLD: f64 = 0.2;
pub const ADAPT_LOW_IGNORE_MULTIPLIER: f64 = 0.7;

/// Iter R7: pure adapter. Given the configured cooldown, the recent ignore
/// ratio, and the sample size that produced it, return the cooldown the
/// gate should actually enforce. Three-band step function:
/// - sample_count < `FEEDBACK_ADAPT_MIN_SAMPLES` → base unchanged
/// - ratio > `ADAPT_HIGH_IGNORE_THRESHOLD` → base × `ADAPT_HIGH_IGNORE_MULTIPLIER`
/// - ratio < `ADAPT_LOW_IGNORE_THRESHOLD` → base × `ADAPT_LOW_IGNORE_MULTIPLIER`
/// - else (mid band) → base unchanged
///
/// Steps not a smooth curve because we want the adaptation to be auditable —
/// a panel reader should be able to compute the result by hand from the
/// ratio chip without evaluating a polynomial.
pub fn adapted_cooldown_seconds(
    base_cooldown_secs: u64,
    ignore_ratio: f64,
    sample_count: usize,
) -> u64 {
    if sample_count < FEEDBACK_ADAPT_MIN_SAMPLES {
        return base_cooldown_secs;
    }
    if ignore_ratio > ADAPT_HIGH_IGNORE_THRESHOLD {
        return ((base_cooldown_secs as f64) * ADAPT_HIGH_IGNORE_MULTIPLIER) as u64;
    }
    if ignore_ratio < ADAPT_LOW_IGNORE_THRESHOLD {
        return ((base_cooldown_secs as f64) * ADAPT_LOW_IGNORE_MULTIPLIER) as u64;
    }
    base_cooldown_secs
}

/// Format a feedback hint for the proactive prompt from the most recent
/// entry. Empty list → empty string. Single entry → one-line nudge that the
/// LLM can absorb. Pure / testable.
pub fn format_feedback_hint(entries: &[FeedbackEntry]) -> String {
    let Some(latest) = entries.last() else {
        return String::new();
    };
    match latest.kind {
        FeedbackKind::Replied => format!(
            "上次你说「{}」，用户回复了 — 这次开口可以接着话题或换个新角度。",
            latest.excerpt
        ),
        FeedbackKind::Ignored => format!(
            "上次你说「{}」，用户没回应 — 这次开口要更有钩子或干脆放短一点（甚至选择沉默）。",
            latest.excerpt
        ),
        FeedbackKind::Dismissed => format!(
            "上次你说「{}」，用户**主动点掉了**气泡 — 比单纯没回应更明显的不感兴趣信号。这次开口要么换完全不同的话题，要么干脆沉默。",
            latest.excerpt
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(kind: FeedbackKind, excerpt: &str) -> FeedbackEntry {
        FeedbackEntry {
            timestamp: "2026-05-03T12:00:00+08:00".to_string(),
            kind,
            excerpt: excerpt.to_string(),
        }
    }

    #[test]
    fn format_line_truncates_long_excerpt() {
        let s = "0123456789".repeat(10); // 100 chars
        let line = format_line("2026-05-03T12:00:00+08:00", FeedbackKind::Replied, &s);
        assert!(line.contains("replied"));
        // 40-char head + …; full 100 should not appear.
        assert!(!line.contains(&s));
        assert!(line.contains("…"));
    }

    #[test]
    fn format_line_flattens_newlines() {
        // Newlines inside the excerpt would break per-line parsing.
        let line = format_line(
            "2026-05-03T12:00:00+08:00",
            FeedbackKind::Ignored,
            "a\nb\rc",
        );
        assert!(!line.contains('\n'));
        assert!(!line.contains('\r'));
        assert!(line.contains("a b c") || line.contains("a b  c"));
    }

    #[test]
    fn parse_line_round_trips_replied_and_ignored() {
        let l1 = format_line("2026-05-03T12:00:00+08:00", FeedbackKind::Replied, "hello");
        let p1 = parse_line(&l1).expect("must parse");
        assert_eq!(p1.kind, FeedbackKind::Replied);
        assert_eq!(p1.excerpt, "hello");
        assert_eq!(p1.timestamp, "2026-05-03T12:00:00+08:00");

        let l2 = format_line("2026-05-03T13:00:00+08:00", FeedbackKind::Ignored, "world");
        let p2 = parse_line(&l2).expect("must parse");
        assert_eq!(p2.kind, FeedbackKind::Ignored);
        assert_eq!(p2.excerpt, "world");
    }

    #[test]
    fn parse_line_rejects_unknown_kind_and_malformed() {
        assert!(parse_line("2026-05-03T12:00:00+08:00 weird | text").is_none());
        assert!(parse_line("no separator").is_none());
        assert!(parse_line("").is_none());
    }

    #[test]
    fn format_feedback_hint_empty_returns_empty() {
        assert_eq!(format_feedback_hint(&[]), "");
    }

    #[test]
    fn format_feedback_hint_replied_mentions_response() {
        let h = format_feedback_hint(&[entry(FeedbackKind::Replied, "今天忙吗？")]);
        assert!(h.contains("用户回复了"));
        assert!(h.contains("今天忙吗"));
    }

    #[test]
    fn format_feedback_hint_ignored_mentions_no_response() {
        let h = format_feedback_hint(&[entry(FeedbackKind::Ignored, "在忙工作？")]);
        assert!(h.contains("没回应") || h.contains("忽略"));
        assert!(
            h.contains("放短") || h.contains("沉默") || h.contains("钩子"),
            "must hint at adjustment direction"
        );
    }

    #[test]
    fn feedback_kind_serializes_as_lowercase_for_frontend() {
        // Iter R6: PanelDebug's feedback timeline matches on the literal
        // strings "replied" / "ignored" to render the pill color. If someone
        // changes the variant names or removes the rename_all, this test
        // fails before the panel renders blank pills.
        assert_eq!(
            serde_json::to_string(&FeedbackKind::Replied).unwrap(),
            "\"replied\""
        );
        assert_eq!(
            serde_json::to_string(&FeedbackKind::Ignored).unwrap(),
            "\"ignored\""
        );
    }

    #[test]
    fn feedback_entry_serializes_with_all_three_fields() {
        // Sanity that timestamp + kind + excerpt all reach the panel.
        let e = entry(FeedbackKind::Replied, "hello world");
        let json = serde_json::to_value(&e).unwrap();
        assert_eq!(
            json["timestamp"].as_str(),
            Some("2026-05-03T12:00:00+08:00")
        );
        assert_eq!(json["kind"].as_str(), Some("replied"));
        assert_eq!(json["excerpt"].as_str(), Some("hello world"));
    }

    #[test]
    fn negative_signal_ratio_returns_none_for_empty_input() {
        assert_eq!(negative_signal_ratio(&[]), None);
    }

    #[test]
    fn negative_signal_ratio_counts_correctly() {
        // 3 ignored / 5 total = 0.6.
        let entries = vec![
            entry(FeedbackKind::Ignored, "a"),
            entry(FeedbackKind::Ignored, "b"),
            entry(FeedbackKind::Replied, "c"),
            entry(FeedbackKind::Replied, "d"),
            entry(FeedbackKind::Ignored, "e"),
        ];
        let (ratio, n) = negative_signal_ratio(&entries).expect("non-empty must yield Some");
        assert_eq!(n, 5);
        assert!((ratio - 0.6).abs() < 1e-9, "expected 0.6, got {}", ratio);
    }

    #[test]
    fn negative_signal_ratio_handles_all_replied() {
        let entries = vec![
            entry(FeedbackKind::Replied, "a"),
            entry(FeedbackKind::Replied, "b"),
        ];
        let (ratio, n) = negative_signal_ratio(&entries).unwrap();
        assert_eq!(n, 2);
        assert_eq!(ratio, 0.0);
    }

    #[test]
    fn negative_signal_ratio_handles_all_ignored() {
        let entries = vec![entry(FeedbackKind::Ignored, "a"); 3];
        let (ratio, n) = negative_signal_ratio(&entries).unwrap();
        assert_eq!(n, 3);
        assert_eq!(ratio, 1.0);
    }

    #[test]
    fn negative_signal_ratio_counts_dismissed_alongside_ignored() {
        // R1b: 2 ignored + 1 dismissed = 3 negative / 5 total = 0.6.
        let entries = vec![
            entry(FeedbackKind::Ignored, "a"),
            entry(FeedbackKind::Dismissed, "b"),
            entry(FeedbackKind::Replied, "c"),
            entry(FeedbackKind::Replied, "d"),
            entry(FeedbackKind::Ignored, "e"),
        ];
        let (ratio, n) = negative_signal_ratio(&entries).unwrap();
        assert_eq!(n, 5);
        assert!((ratio - 0.6).abs() < 1e-9, "expected 0.6, got {}", ratio);
    }

    #[test]
    fn negative_signal_ratio_handles_all_dismissed() {
        let entries = vec![entry(FeedbackKind::Dismissed, "a"); 4];
        let (ratio, n) = negative_signal_ratio(&entries).unwrap();
        assert_eq!(n, 4);
        assert_eq!(ratio, 1.0);
    }

    #[test]
    fn dismissed_round_trips_through_format_and_parse() {
        // R1b: log line written today must be readable on next process start.
        let line = format_line("2026-05-04T12:00:00+08:00", FeedbackKind::Dismissed, "x");
        let parsed = parse_line(&line).unwrap();
        assert_eq!(parsed.kind, FeedbackKind::Dismissed);
        assert_eq!(parsed.excerpt, "x");
    }

    #[test]
    fn format_feedback_hint_handles_dismissed_with_stronger_phrasing() {
        let h = format_feedback_hint(&[entry(FeedbackKind::Dismissed, "在忙工作？")]);
        assert!(h.contains("在忙工作？"));
        // Stronger phrasing — calls out active dismissal explicitly.
        assert!(h.contains("主动点掉"));
    }

    #[test]
    fn adapted_cooldown_returns_base_below_min_samples() {
        // Below FEEDBACK_ADAPT_MIN_SAMPLES (5), the ratio is too noisy to act on
        // — return base regardless of value.
        assert_eq!(adapted_cooldown_seconds(1800, 0.9, 0), 1800);
        assert_eq!(adapted_cooldown_seconds(1800, 0.9, 4), 1800);
        assert_eq!(adapted_cooldown_seconds(1800, 0.0, 4), 1800);
    }

    #[test]
    fn adapted_cooldown_doubles_on_high_ignore_ratio() {
        // > 0.6 and at-or-above sample threshold → 2× base.
        assert_eq!(adapted_cooldown_seconds(1800, 0.7, 5), 3600);
        assert_eq!(adapted_cooldown_seconds(1800, 0.99, 20), 3600);
    }

    #[test]
    fn adapted_cooldown_shrinks_on_low_ignore_ratio() {
        // < 0.2 → 0.7× base.
        assert_eq!(adapted_cooldown_seconds(1800, 0.1, 5), 1260);
        assert_eq!(adapted_cooldown_seconds(1800, 0.0, 10), 1260);
    }

    #[test]
    fn adapted_cooldown_keeps_base_in_mid_band() {
        // Between thresholds [0.2, 0.6] → unchanged.
        assert_eq!(adapted_cooldown_seconds(1800, 0.3, 5), 1800);
        assert_eq!(adapted_cooldown_seconds(1800, 0.5, 10), 1800);
        assert_eq!(adapted_cooldown_seconds(1800, 0.6, 10), 1800);
        assert_eq!(adapted_cooldown_seconds(1800, 0.2, 10), 1800);
    }

    #[test]
    fn adapted_cooldown_handles_zero_base() {
        // base=0 means cooldown disabled — adapter must not bring it back.
        // Otherwise a high-ignore session would re-enable cooldown the user
        // intentionally turned off in settings.
        assert_eq!(adapted_cooldown_seconds(0, 0.9, 10), 0);
        assert_eq!(adapted_cooldown_seconds(0, 0.1, 10), 0);
    }

    #[test]
    fn format_feedback_hint_uses_latest_entry_only() {
        // Older entries are background context; only the freshest gets quoted.
        let entries = vec![
            entry(FeedbackKind::Ignored, "OLD utterance"),
            entry(FeedbackKind::Replied, "NEW utterance"),
        ];
        let h = format_feedback_hint(&entries);
        assert!(h.contains("NEW utterance"));
        assert!(!h.contains("OLD utterance"));
    }
}
