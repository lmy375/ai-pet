//! Pet mood/state — the LLM-managed feeling that persists across turns.
//!
//! Lives as a single memory entry at `ai_insights/current_mood`, written by the model
//! itself via the `memory_edit` tool with a `[motion: X] free text` description. Rust
//! never bootstraps or rewrites the entry — it only reads, parses, and reports back.
//!
//! All four LLM entry points (proactive, chat, telegram, consolidate) consume mood
//! through these helpers so behavior stays symmetric.

use crate::commands::debug::{write_log, LogStore};
use crate::commands::memory;

/// Memory category + title where the pet's evolving mood/state is stored. Read on every
/// LLM turn for context, and the model is instructed to update it via `memory_edit` so
/// personality state persists across iterations.
pub const MOOD_CATEGORY: &str = "ai_insights";
pub const MOOD_TITLE: &str = "current_mood";

/// Read the pet's current mood/state from memory (`ai_insights/current_mood`). Returns the
/// item's description if present, otherwise `None`. The LLM bootstraps this on first
/// proactive turn via `memory_edit` — we never write it from Rust to keep the source of
/// truth in the model's hands.
pub fn read_current_mood() -> Option<String> {
    let index = memory::memory_list(Some(MOOD_CATEGORY.to_string())).ok()?;
    let cat = index.categories.get(MOOD_CATEGORY)?;
    cat.items
        .iter()
        .find(|i| i.title == MOOD_TITLE)
        .map(|i| i.description.clone())
}

/// Parse `current_mood` into (mood_text, motion_group). The LLM is instructed to write
/// descriptions in the form `[motion: X] free-form text` where X is one of the Live2D
/// motion group names. If the prefix is absent, motion is None and text is the raw value.
///
/// Returns None if no mood is recorded yet.
pub fn read_current_mood_parsed() -> Option<(String, Option<String>)> {
    let raw = read_current_mood()?;
    Some(parse_mood_string(&raw))
}

/// Pure-function variant of the parsing — extracted for unit testing without touching the
/// memory store. Splits an optional `[motion: X]` prefix off the raw description; if the
/// prefix is missing, malformed, or carries a too-long tag, falls back to the raw text
/// with motion=None. Returns owned strings so callers don't have to manage lifetimes
/// against the source.
pub fn parse_mood_string(raw: &str) -> (String, Option<String>) {
    let trimmed = raw.trim_start();
    if let Some(after_open) = trimmed.strip_prefix("[motion:") {
        if let Some(close_idx) = after_open.find(']') {
            let motion = after_open[..close_idx].trim().to_string();
            let text = after_open[close_idx + 1..].trim().to_string();
            // Defend against empty or impossibly long tags from a confused model.
            if !motion.is_empty() && motion.len() <= 16 {
                return (text, Some(motion));
            }
        }
    }
    (raw.to_string(), None)
}

/// Shared post-turn mood read used by every LLM entry point (proactive, chat, telegram,
/// consolidate). Reads the current mood, parses the optional `[motion: X]` prefix, and
/// emits a single line of compliance telemetry when the prefix is missing. `source` is
/// the human-readable label that prefixes the log line so the user can tell which
/// pipeline produced the warning.
pub fn read_mood_for_event(
    log_store: &LogStore,
    source: &str,
) -> (Option<String>, Option<String>) {
    let parsed = read_current_mood_parsed();
    if let Some((text, None)) = &parsed {
        if !text.trim().is_empty() {
            write_log(
                &log_store.0,
                &format!(
                    "{}: mood missing [motion: X] prefix — frontend will fall back to keyword match",
                    source
                ),
            );
        }
    }
    match parsed {
        Some((t, m)) => (Some(t), m),
        None => (None, None),
    }
}

#[cfg(test)]
mod tests {
    use super::parse_mood_string;

    #[test]
    fn parses_well_formed_prefix() {
        let (text, motion) = parse_mood_string("[motion: Tap] 看用户在写代码，替他高兴");
        assert_eq!(motion.as_deref(), Some("Tap"));
        assert_eq!(text, "看用户在写代码，替他高兴");
    }

    #[test]
    fn allows_extra_whitespace_inside_prefix() {
        let (text, motion) = parse_mood_string("[motion:   Flick3   ]   有点烦躁");
        assert_eq!(motion.as_deref(), Some("Flick3"));
        assert_eq!(text, "有点烦躁");
    }

    #[test]
    fn no_prefix_returns_raw_with_none() {
        let (text, motion) = parse_mood_string("觉得今天过得很平静");
        assert!(motion.is_none());
        assert_eq!(text, "觉得今天过得很平静");
    }

    #[test]
    fn empty_motion_falls_back() {
        let (text, motion) = parse_mood_string("[motion: ] 心情");
        assert!(motion.is_none());
        assert_eq!(text, "[motion: ] 心情");
    }

    #[test]
    fn oversized_motion_falls_back() {
        // 17 chars, exceeds 16 limit — defends against the LLM dumping prose into the slot.
        let (_text, motion) = parse_mood_string("[motion: aaaaaaaaaaaaaaaaa] hi");
        assert!(motion.is_none());
    }

    #[test]
    fn unclosed_bracket_falls_back() {
        let (text, motion) = parse_mood_string("[motion: Tap 心情没收尾");
        assert!(motion.is_none());
        assert_eq!(text, "[motion: Tap 心情没收尾");
    }

    #[test]
    fn empty_text_after_prefix() {
        let (text, motion) = parse_mood_string("[motion: Idle]");
        assert_eq!(motion.as_deref(), Some("Idle"));
        assert_eq!(text, "");
    }

    #[test]
    fn handles_leading_whitespace() {
        let (text, motion) = parse_mood_string("   [motion: Tap] hello");
        assert_eq!(motion.as_deref(), Some("Tap"));
        assert_eq!(text, "hello");
    }
}
