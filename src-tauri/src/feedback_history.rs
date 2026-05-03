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

/// Cap on lines retained in the log. About 30 days at the typical proactive
/// cadence; older entries roll off so the file stays bounded without a
/// background pruner.
pub const FEEDBACK_HISTORY_CAP: usize = 200;

/// How many characters of the previous proactive utterance to keep in each
/// log line. Long enough to distinguish utterances, short enough to keep
/// `recent_feedback` cheap.
pub const FEEDBACK_EXCERPT_CHARS: usize = 40;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedbackKind {
    Replied,
    Ignored,
}

impl FeedbackKind {
    pub fn as_str(self) -> &'static str {
        match self {
            FeedbackKind::Replied => "replied",
            FeedbackKind::Ignored => "ignored",
        }
    }
}

#[derive(Debug, Clone)]
pub struct FeedbackEntry {
    /// ISO-formatted local timestamp. Captured for the future panel feedback
    /// timeline (Iter R4 follow-up); the prompt-hint formatter doesn't read it
    /// today, so silence the dead-code lint without dropping the field.
    #[allow(dead_code)]
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
