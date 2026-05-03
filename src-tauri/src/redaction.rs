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

/// Process-wide redaction counters (Iter Cv). Static rather than living on
/// `ProcessCounters` because `redact_with_settings` is called from sync paths that
/// don't have access to Tauri state (e.g. `inject_mood_note`, `build_persona_hint`).
/// Two atomics are enough — calls = total invocations of `redact_with_settings`;
/// hits = invocations where the input differed from the output (i.e. at least one
/// pattern matched something). Reset via the Tauri command of the same name.
pub static REDACTION_CALLS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub static REDACTION_HITS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Apply redaction using the user's currently configured privacy patterns. Sync
/// wrapper that reads settings on every call so user edits take effect immediately
/// (same model as the env-tool sites in Iter Cx). Failure to read settings yields
/// empty pattern lists — text passes through unchanged rather than blocking.
///
/// Iter Cz: applies substring patterns first, then regex patterns. Two-pass order
/// is deliberate — substring matches are typically more specific (named terms),
/// regex catches structural patterns; running substrings first means specific
/// names get the marker before a wide email regex could swallow context around them.
pub fn redact_with_settings(text: &str) -> String {
    use std::sync::atomic::Ordering;
    REDACTION_CALLS.fetch_add(1, Ordering::Relaxed);
    let (subs, regexes) = crate::commands::settings::get_settings()
        .map(|s| {
            (
                s.privacy.redaction_patterns.clone(),
                s.privacy.regex_patterns.clone(),
            )
        })
        .unwrap_or_default();
    let after_substr = redact_text(text, &subs);
    let after_regex = redact_regex(&after_substr, &regexes);
    if after_regex != text {
        REDACTION_HITS.fetch_add(1, Ordering::Relaxed);
    }
    after_regex
}

#[derive(serde::Serialize)]
pub struct RedactionStats {
    pub calls: u64,
    pub hits: u64,
}

#[tauri::command]
pub fn get_redaction_stats() -> RedactionStats {
    use std::sync::atomic::Ordering;
    RedactionStats {
        calls: REDACTION_CALLS.load(Ordering::Relaxed),
        hits: REDACTION_HITS.load(Ordering::Relaxed),
    }
}

#[tauri::command]
pub fn reset_redaction_stats() {
    use std::sync::atomic::Ordering;
    REDACTION_CALLS.store(0, Ordering::Relaxed);
    REDACTION_HITS.store(0, Ordering::Relaxed);
}

/// Apply regex redaction. Each pattern is compiled fresh per call — there's no
/// shared cache. Invalid patterns are silently skipped (logged at debug level if
/// the caller wants observability later). RE2-style regex semantics (no
/// backreferences, linear time) defend against ReDoS by construction. Empty patterns
/// behave like empty substrings: skipped.
pub fn redact_regex(text: &str, patterns: &[String]) -> String {
    let mut out = text.to_string();
    for pat in patterns {
        let p = pat.trim();
        if p.is_empty() {
            continue;
        }
        // `replace_all` with an empty match is well-defined (matches between every
        // char). We only fire if the regex actually compiled and matches something
        // meaningful — empty pattern is already filtered above.
        if let Ok(re) = regex::Regex::new(p) {
            out = re.replace_all(&out, REDACTION_MARKER).to_string();
        }
    }
    out
}

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

    // ---- Iter Cz: regex pattern redaction ----

    #[test]
    fn redact_regex_empty_patterns_is_identity() {
        assert_eq!(redact_regex("hello world", &[]), "hello world");
    }

    #[test]
    fn redact_regex_skips_blank_patterns() {
        let patterns = vec!["".to_string(), "   ".to_string()];
        assert_eq!(redact_regex("hello world", &patterns), "hello world");
    }

    #[test]
    fn redact_regex_email_pattern() {
        let patterns = vec![r"[\w.+-]+@[\w-]+\.[\w.-]+".to_string()];
        assert_eq!(
            redact_regex("write to alice@example.com later", &patterns),
            "write to (私人) later",
        );
    }

    #[test]
    fn redact_regex_credit_card_pattern() {
        let patterns = vec![r"\b\d{4}-\d{4}-\d{4}-\d{4}\b".to_string()];
        assert_eq!(
            redact_regex("card 1234-5678-9012-3456 to charge", &patterns),
            "card (私人) to charge",
        );
    }

    #[test]
    fn redact_regex_invalid_pattern_silently_skipped() {
        // Unbalanced bracket — re::new returns Err. Should not panic, should not
        // disable other patterns; just ignored.
        let patterns = vec!["[unclosed".to_string(), r"\d{3}".to_string()];
        assert_eq!(redact_regex("call 911", &patterns), "call (私人)");
    }

    #[test]
    fn redact_regex_multiple_patterns_apply_in_order() {
        let patterns = vec![r"\d+".to_string(), r"[a-z]+".to_string()];
        // Numbers first, then lowercase words — both replaced.
        assert_eq!(
            redact_regex("abc 123 xyz", &patterns),
            "(私人) (私人) (私人)"
        );
    }

    #[test]
    fn redact_regex_handles_chinese_text() {
        // 4 consecutive digits anywhere in the string.
        let patterns = vec![r"\d{4}".to_string()];
        assert_eq!(
            redact_regex("订单号 8801 已发货", &patterns),
            "订单号 (私人) 已发货",
        );
    }

    // ---- Iter Cv: redaction stats ----
    //
    // The counters are global statics, so these tests use redact_text / redact_regex
    // directly (which don't touch the counters) plus a small simulation of the bump
    // logic. We avoid asserting on the live REDACTION_CALLS because settings IO and
    // other tests in the same binary will perturb it.

    #[test]
    fn redaction_stats_struct_serializes() {
        let s = RedactionStats { calls: 10, hits: 3 };
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"calls\":10"));
        assert!(json.contains("\"hits\":3"));
    }
}
