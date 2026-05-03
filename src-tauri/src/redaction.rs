//! Local privacy filter (Iter Cx / route C).
//!
//! When environment-aware tools (`get_active_window`, `get_upcoming_events`, etc.)
//! return data into the LLM prompt, app titles and event subjects can leak personal
//! information (Slack DM partner names, calendar invitee company names, etc.). This
//! module provides a single configurable substring-replacement pass: user lists
//! patterns in settings, anywhere a pattern matches in tool output gets replaced
//! with `(私人)` before the LLM sees it.
//!
//! Design choices:
//! - Substring rather than regex: trivially safe, no ReDoS, user-friendly.
//! - Case-insensitive: typing "slack" also matches "Slack" without listing both.
//! - Empty pattern strings are skipped (avoid the "" → empty-replacement infinite-
//!   loop trap).
//! - Order of patterns is the order applied — earlier patterns can mask substrings
//!   that later ones would have matched.

/// Replacement marker the LLM (and the user reading logs) sees in place of any
/// redacted substring. Visible enough to recognize as redaction; short enough not
/// to blow up sentences.
pub const REDACTION_MARKER: &str = "(私人)";

/// Apply substring redaction. Patterns matching case-insensitively in `text` are
/// replaced with `REDACTION_MARKER`. Returns owned String even when nothing
/// matched — caller doesn't need a borrow vs. owned branch.
pub fn redact_text(text: &str, patterns: &[String]) -> String {
    let mut out = text.to_string();
    for pat in patterns {
        let p = pat.trim();
        if p.is_empty() {
            continue;
        }
        out = replace_case_insensitive(&out, p, REDACTION_MARKER);
    }
    out
}

/// Pure: case-insensitive substring replacement. Walks the haystack scanning the
/// lowercased view; emits non-matching chars verbatim and `replacement` for each
/// match. Avoids regex dep and works on UTF-8 directly (Chinese / emoji safe).
fn replace_case_insensitive(haystack: &str, needle: &str, replacement: &str) -> String {
    if needle.is_empty() {
        return haystack.to_string();
    }
    let h_lower = haystack.to_lowercase();
    let n_lower = needle.to_lowercase();
    let mut out = String::with_capacity(haystack.len());
    let mut idx = 0usize;
    let bytes = haystack.as_bytes();
    while idx < haystack.len() {
        if h_lower[idx..].starts_with(&n_lower) {
            out.push_str(replacement);
            idx += n_lower.len();
        } else {
            // Advance by one char (UTF-8 aware): find next char boundary.
            let mut step = 1;
            while idx + step < haystack.len() && !haystack.is_char_boundary(idx + step) {
                step += 1;
            }
            // Copy the original-case bytes for this char.
            out.push_str(std::str::from_utf8(&bytes[idx..idx + step]).unwrap_or(""));
            idx += step;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_empty_patterns_returns_input_unchanged() {
        assert_eq!(redact_text("hello world", &[]), "hello world");
    }

    #[test]
    fn redact_skips_blank_patterns() {
        // Empty / whitespace-only patterns must not produce nonsense replacements.
        let patterns = vec!["".to_string(), "   ".to_string()];
        assert_eq!(redact_text("hello world", &patterns), "hello world");
    }

    #[test]
    fn redact_substring_match_is_case_insensitive() {
        let patterns = vec!["slack".to_string()];
        assert_eq!(
            redact_text("Working in Slack today", &patterns),
            "Working in (私人) today",
        );
    }

    #[test]
    fn redact_handles_multiple_patterns_in_order() {
        let patterns = vec!["foo".to_string(), "bar".to_string()];
        assert_eq!(
            redact_text("FOO BAR baz foobar", &patterns),
            "(私人) (私人) baz (私人)(私人)",
        );
    }

    #[test]
    fn redact_handles_chinese_text() {
        let patterns = vec!["公司名".to_string()];
        assert_eq!(
            redact_text("今天和公司名的同事开会", &patterns),
            "今天和(私人)的同事开会",
        );
    }

    #[test]
    fn redact_handles_emoji_safe_iteration() {
        // Multi-byte UTF-8 chars must not be corrupted when no pattern matches.
        let patterns = vec!["nope".to_string()];
        assert_eq!(redact_text("hi 👋 there", &patterns), "hi 👋 there");
    }

    #[test]
    fn redact_overlapping_patterns_use_first_match() {
        // "Slack" matches first; "lack" would also match in raw text, but the slot
        // is already redacted — replacement output isn't re-scanned.
        let patterns = vec!["Slack".to_string(), "lack".to_string()];
        let out = redact_text("In Slack now", &patterns);
        assert_eq!(out, "In (私人) now");
    }

    #[test]
    fn redact_repeated_occurrences_all_replaced() {
        let patterns = vec!["x".to_string()];
        assert_eq!(redact_text("xxx", &patterns), "(私人)(私人)(私人)");
    }
}
