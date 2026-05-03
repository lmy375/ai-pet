//! Append-only history of the pet's own proactive utterances.
//!
//! Solves a specific problem: every proactive turn loads recent session messages and
//! injects them into the prompt — but session history can be long, get trimmed by the
//! `chat.max_context_messages` cap, or be a fresh session. The pet then forgets it just
//! said "good morning" 30 minutes ago and says it again. This module gives the model a
//! short, dedicated, deterministic record of "what I, the pet, said last".
//!
//! File: `~/.config/pet/speech_history.log`. One line per utterance, ISO timestamp space
//! text — newlines in the text are flattened to spaces. Trimmed to `SPEECH_HISTORY_CAP`
//! entries on every write so it never grows unbounded.

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::log_rotation::rotate_if_needed;

/// Hard cap on retained entries. Far more than the prompt ever surfaces (5–10) — the
/// extra slack lets future features (e.g. a panel "what did the pet say lately?" view)
/// reach further back without re-architecting. `pub` so callers can detect when
/// `count_speeches` has saturated (a "50+" affordance vs reading 50 as the literal
/// lifetime number).
pub const SPEECH_HISTORY_CAP: usize = 50;
/// Byte ceiling — defense in depth on top of the line-count trim. A misbehaving LLM that
/// emits a megabyte-long "single utterance" can't blow up the file: rotation kicks in
/// and the next write starts a fresh log.
const SPEECH_HISTORY_MAX_BYTES: u64 = 100_000;

fn history_path() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("pet").join("speech_history.log"))
}

/// Tiny sidecar that holds the lifetime count of proactive utterances as a single
/// integer. We need this because `speech_history.log` is trimmed at SPEECH_HISTORY_CAP
/// and so its line count saturates; this file just keeps incrementing forever.
fn count_path() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("pet").join("speech_count.txt"))
}

/// Per-day bucket file: `{ "YYYY-MM-DD": count, ... }`. Lets the panel show "今天开口 N
/// 次" alongside the lifetime total without scanning speech_history.log timestamps every
/// tick. Pruned to `DAILY_RETAIN_DAYS` so it can't grow unbounded over years of use.
fn daily_path() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("pet").join("speech_daily.json"))
}

/// How many trailing days to retain in the per-day bucket file. Anything older is dropped
/// on the next write. 90 is more than the panel ever surfaces (today only, for now) but
/// gives future "last 7d" / "last 30d" features room to read back without re-architecting.
const DAILY_RETAIN_DAYS: usize = 90;

/// Append a new utterance to the history file, trimming to `SPEECH_HISTORY_CAP` entries
/// total. Best-effort — IO errors are silently ignored so a hosed disk doesn't break the
/// pet's actual speaking flow.
pub async fn record_speech(text: &str) {
    let _ = record_speech_inner(text).await;
}

async fn record_speech_inner(text: &str) -> std::io::Result<()> {
    let Some(path) = history_path() else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    // Rotate first so an oversized file doesn't get re-read into memory before we replace
    // it. After rotation the next read starts fresh; trimming to SPEECH_HISTORY_CAP still
    // applies to the new generation.
    let _ = rotate_if_needed(&path, SPEECH_HISTORY_MAX_BYTES).await;
    let existing = tokio::fs::read_to_string(&path).await.unwrap_or_default();
    let mut entries: Vec<String> = existing
        .lines()
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect();
    let ts = chrono::Local::now()
        .format("%Y-%m-%dT%H:%M:%S%:z")
        .to_string();
    let flat = text.replace(['\n', '\r'], " ");
    entries.push(format!("{} {}", ts, flat));
    if entries.len() > SPEECH_HISTORY_CAP {
        let drop = entries.len() - SPEECH_HISTORY_CAP;
        entries.drain(0..drop);
    }
    let mut content = entries.join("\n");
    content.push('\n');
    tokio::fs::write(&path, content).await?;
    // Best-effort lifetime counter bump — failure here doesn't fail the speech write.
    let _ = bump_lifetime_count().await;
    // Best-effort per-day bucket bump — same rationale.
    let _ = bump_today_count().await;
    Ok(())
}

/// Read up to the last `n` entries from the history file. Empty vector when the file is
/// missing, unreadable, or `n == 0`.
pub async fn recent_speeches(n: usize) -> Vec<String> {
    if n == 0 {
        return vec![];
    }
    let Some(path) = history_path() else {
        return vec![];
    };
    let content = tokio::fs::read_to_string(&path).await.unwrap_or_default();
    parse_recent(&content, n)
}

/// Pure parser — gives the last `n` non-empty lines from `content` in original order
/// (oldest of the kept window first, newest last). Extracted so unit tests can exercise
/// the slicing without touching the filesystem.
pub fn parse_recent(content: &str, n: usize) -> Vec<String> {
    if n == 0 {
        return vec![];
    }
    let lines: Vec<&str> = content.lines().filter(|l| !l.is_empty()).collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].iter().map(|s| s.to_string()).collect()
}

/// Strip the leading ISO-8601 timestamp from a recorded line, returning just the text.
/// If the line doesn't look like "<ts> <text>" (no space), returns the whole line.
pub fn strip_timestamp(line: &str) -> &str {
    line.split_once(' ').map(|(_, rest)| rest).unwrap_or(line)
}

/// Iter R14: filter speech_history lines to those whose timestamp's local
/// date matches `target_date`. Returns up to `max` most recent matching
/// lines (oldest of the kept window first, mirroring `parse_recent`).
/// Pure — caller passes the file content + the target NaiveDate so tests
/// don't depend on system clock.
///
/// Used by the proactive prompt's cross-day-thread hint: at the first
/// proactive turn of a new day, surface yesterday's last 2 utterances so
/// the pet can pick up where it left off ("昨天我们最后聊到 X，今天怎么样？")
/// instead of starting cold every morning.
///
/// Lines with malformed / non-ISO timestamps are silently skipped — the
/// log format is "<ISO ts> <text>"; anything that doesn't start that way
/// can't be filtered by date and is treated as not-matching.
pub fn speeches_for_date(content: &str, target_date: chrono::NaiveDate, max: usize) -> Vec<String> {
    if max == 0 {
        return vec![];
    }
    let mut matches: Vec<String> = content
        .lines()
        .filter(|l| !l.is_empty())
        .filter(|line| {
            let Some((ts, _rest)) = line.split_once(' ') else {
                return false;
            };
            let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts) else {
                return false;
            };
            dt.with_timezone(&chrono::Local).date_naive() == target_date
        })
        .map(String::from)
        .collect();
    let start = matches.len().saturating_sub(max);
    if start > 0 {
        matches.drain(0..start);
    }
    matches
}

/// Iter R14: async wrapper — read the speech history file and return up to
/// `max` lines matching `target_date`. Empty Vec on missing file or no
/// matches. Caller-side production path uses `chrono::Local::now() - 1
/// day` to fetch yesterday's tail.
pub async fn speeches_for_date_async(target_date: chrono::NaiveDate, max: usize) -> Vec<String> {
    let Some(path) = history_path() else {
        return vec![];
    };
    let content = tokio::fs::read_to_string(&path).await.unwrap_or_default();
    speeches_for_date(&content, target_date, max)
}

/// Iter R11: coarse Chinese-friendly topic-redundancy detector. Slides a
/// `ngram_size`-character window across each line (after `strip_timestamp`)
/// and counts how many *distinct lines* each ngram appears in. Returns the
/// ngram appearing in the most lines, but only if it crosses
/// `min_distinct_lines`. Returns `None` when no ngram qualifies.
///
/// The pet uses this on its own recent speeches to detect "I keep saying
/// the same thing" — the proactive prompt can then nudge the LLM with
/// "你最近 N 句都聊到了 X，这次换个角度". 4-char windows are roughly
/// word-level for Chinese without needing tokenization.
///
/// Pure / testable so the wiring layer doesn't need to mock IO. Skips
/// ngrams containing whitespace (avoids matching trivial "了 " or "  X"
/// patterns that span word boundaries). Also skips ngrams that are pure
/// ASCII punctuation / single repeated char.
pub fn detect_repeated_topic(
    lines: &[String],
    ngram_size: usize,
    min_distinct_lines: usize,
) -> Option<String> {
    if lines.is_empty() || ngram_size == 0 || min_distinct_lines == 0 {
        return None;
    }
    use std::collections::HashMap;
    // Map ngram → set of line indices it appears in (use bitset via u64
    // for small line counts, but HashMap<String, HashSet<usize>> is simplest).
    let mut counts: HashMap<String, std::collections::HashSet<usize>> = HashMap::new();
    for (idx, raw) in lines.iter().enumerate() {
        let text = strip_timestamp(raw);
        let chars: Vec<char> = text.chars().collect();
        if chars.len() < ngram_size {
            continue;
        }
        for i in 0..=(chars.len() - ngram_size) {
            let window: String = chars[i..i + ngram_size].iter().collect();
            // Skip whitespace-bearing windows — those span word/sentence
            // boundaries and produce noisy matches like "了 哎".
            if window.chars().any(|c| c.is_whitespace()) {
                continue;
            }
            // Skip windows that are all the same char (e.g. "...." or "了了了了") —
            // those are formatting artifacts, not topics.
            let first = window.chars().next();
            if window.chars().all(|c| Some(c) == first) {
                continue;
            }
            counts.entry(window).or_default().insert(idx);
        }
    }
    // Pick the ngram with the highest distinct-line count, tie-break by
    // shorter alphabetical for stability across runs.
    let (best_ngram, best_set) = counts.into_iter().max_by(|a, b| {
        a.1.len().cmp(&b.1.len()).then_with(|| b.0.cmp(&a.0)) // alphabetical reverse for ties so smaller wins
    })?;
    if best_set.len() >= min_distinct_lines {
        Some(best_ngram)
    } else {
        None
    }
}

/// Iter R19: register-variance nudge — analyze recent speech char counts
/// and surface a hint when the pet is stuck in one length register. Real
/// friends mix terse "嘿" with longer reach-outs; if every recent line is
/// either ≥ `LONG_CHAR_THRESHOLD` chars or ≤ `SHORT_CHAR_THRESHOLD` chars,
/// the LLM gets a one-line nudge to break the pattern.
///
/// Empty string when:
/// - fewer than `MIN_SAMPLES` lines (not enough signal — early sessions)
/// - mixed register (some short, some long — already varying)
///
/// Char counting uses `chars().count()`, not `len()`, so 1 汉字 = 1 char.
/// Otherwise 30-char Chinese line would register as 90-byte "very long"
/// when it's actually conversational length.
pub const SPEECH_LENGTH_MIN_SAMPLES: usize = 3;
pub const SPEECH_LENGTH_LONG_THRESHOLD: usize = 25;
pub const SPEECH_LENGTH_SHORT_THRESHOLD: usize = 8;

/// Iter R20: classification of recent-speeches register. Surfaces both to
/// the prompt builder (R19) and to the panel tone strip (R20) — same
/// classifier, two consumers. None when fewer than `MIN_SAMPLES` nonzero
/// lines (not enough data to classify).
#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
pub struct SpeechRegisterSummary {
    /// `"long"` (all samples ≥ LONG_THRESHOLD) / `"short"` (all ≤ SHORT_THRESHOLD)
    /// / `"mixed"` (varying — neither extreme). Frontend renders different
    /// chip tone per kind.
    pub kind: &'static str,
    pub mean_chars: usize,
    pub samples: usize,
}

/// Pure classifier — strips timestamps, filters empty lines, returns kind +
/// mean + sample count. The R19 prompt-hint formatter calls this; the R20
/// panel snapshot also calls this; both branches share one rule.
pub fn classify_speech_register(lines: &[String]) -> Option<SpeechRegisterSummary> {
    if lines.len() < SPEECH_LENGTH_MIN_SAMPLES {
        return None;
    }
    let counts: Vec<usize> = lines
        .iter()
        .map(|raw| strip_timestamp(raw).chars().count())
        .filter(|&n| n > 0)
        .collect();
    if counts.len() < SPEECH_LENGTH_MIN_SAMPLES {
        return None;
    }
    let mean_chars = counts.iter().sum::<usize>() / counts.len();
    let kind = if counts.iter().all(|&n| n >= SPEECH_LENGTH_LONG_THRESHOLD) {
        "long"
    } else if counts.iter().all(|&n| n <= SPEECH_LENGTH_SHORT_THRESHOLD) {
        "short"
    } else {
        "mixed"
    };
    Some(SpeechRegisterSummary {
        kind,
        mean_chars,
        samples: counts.len(),
    })
}

pub fn format_speech_length_hint(lines: &[String]) -> String {
    let Some(summary) = classify_speech_register(lines) else {
        return String::new();
    };
    match summary.kind {
        "long" => format!(
            "你最近 {} 句开口都偏长（平均 {} 字），这次试更短的关心 — 一句话甚至几个字也行。",
            summary.samples, summary.mean_chars
        ),
        "short" => format!(
            "你最近 {} 句开口都偏短（平均 {} 字），这次可以多花两句关心一下细节。",
            summary.samples, summary.mean_chars
        ),
        // mixed — already varying, no nudge.
        _ => String::new(),
    }
}

/// Tauri command exposing the most recent N speech entries to the panel UI. Each entry
/// is the raw "<ts> <text>" line — the frontend strips the timestamp itself for display
/// flexibility (could show as relative time later). Default n=10 if not supplied.
#[tauri::command]
pub async fn get_recent_speeches(n: Option<usize>) -> Vec<String> {
    recent_speeches(n.unwrap_or(10)).await
}

/// Tauri command exposing the persistent lifetime speech count for the panel stats
/// header. Thin wrapper over `lifetime_speech_count` so the frontend can `invoke` it
/// without going through `get_tone_snapshot` (which mixes a dozen other fields).
#[tauri::command]
pub async fn get_lifetime_speech_count() -> u64 {
    lifetime_speech_count().await
}

/// Number of non-empty lines currently in the speech_history.log file. Caps at
/// `SPEECH_HISTORY_CAP` because of trim-on-write. Useful for "show last N speeches"
/// but not for true lifetime stats — see `lifetime_speech_count` for that.
pub async fn count_speeches() -> usize {
    let Some(path) = history_path() else {
        return 0;
    };
    let content = tokio::fs::read_to_string(&path).await.unwrap_or_default();
    content.lines().filter(|l| !l.trim().is_empty()).count()
}

/// Lifetime count of proactive utterances, persisted across restarts in a sidecar file.
/// Returns 0 on a brand-new install. For an existing install that's upgrading from the
/// pre-counter version, the counter file won't exist yet; in that case we bootstrap
/// from `count_speeches` so users don't see a regression to 0 after upgrading. After
/// the first bump the sidecar file always exists and takes precedence.
pub async fn lifetime_speech_count() -> u64 {
    let Some(path) = count_path() else {
        return 0;
    };
    if let Ok(s) = tokio::fs::read_to_string(&path).await {
        if let Ok(n) = s.trim().parse::<u64>() {
            return n;
        }
    }
    // Bootstrap path — file missing or malformed.
    count_speeches().await as u64
}

/// Best-effort: increment the persistent lifetime counter by 1.
async fn bump_lifetime_count() -> std::io::Result<()> {
    let Some(path) = count_path() else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let current = lifetime_speech_count().await;
    tokio::fs::write(&path, format!("{}\n", current + 1)).await
}

/// Pure: parse the daily-bucket JSON. Malformed input returns an empty map (caller will
/// then write a fresh map on the next bump — a corrupt file self-heals after one speech).
pub fn parse_daily(content: &str) -> BTreeMap<String, u64> {
    serde_json::from_str(content).unwrap_or_default()
}

/// Pure: drop entries whose date keys come before `today - retain_days`. Uses string
/// comparison because YYYY-MM-DD sorts lexicographically. Non-parseable keys are kept —
/// the caller hasn't written them, so they're either future migrations or user-edited.
pub fn prune_daily(
    mut map: BTreeMap<String, u64>,
    today: chrono::NaiveDate,
    retain_days: usize,
) -> BTreeMap<String, u64> {
    let cutoff = today - chrono::Duration::days(retain_days as i64);
    let cutoff_str = cutoff.format("%Y-%m-%d").to_string();
    map.retain(
        |k, _| match chrono::NaiveDate::parse_from_str(k, "%Y-%m-%d") {
            Ok(_) => k.as_str() >= cutoff_str.as_str(),
            Err(_) => true,
        },
    );
    map
}

/// Today's date in `YYYY-MM-DD` form using local time — same timezone as the speech log
/// timestamps so "今天" matches what the user's clock shows.
fn today_key() -> String {
    chrono::Local::now()
        .date_naive()
        .format("%Y-%m-%d")
        .to_string()
}

/// Best-effort: increment today's bucket and prune any entries beyond the retain window.
async fn bump_today_count() -> std::io::Result<()> {
    let Some(path) = daily_path() else {
        return Ok(());
    };
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let existing = tokio::fs::read_to_string(&path).await.unwrap_or_default();
    let mut map = parse_daily(&existing);
    let key = today_key();
    *map.entry(key).or_insert(0) += 1;
    let pruned = prune_daily(map, chrono::Local::now().date_naive(), DAILY_RETAIN_DAYS);
    let json = serde_json::to_string(&pruned).unwrap_or_else(|_| "{}".to_string());
    tokio::fs::write(&path, json).await
}

/// Number of proactive utterances recorded today (local time). Returns 0 when the file
/// doesn't exist or today's bucket is missing.
pub async fn today_speech_count() -> u64 {
    let Some(path) = daily_path() else {
        return 0;
    };
    let content = tokio::fs::read_to_string(&path).await.unwrap_or_default();
    let map = parse_daily(&content);
    map.get(&today_key()).copied().unwrap_or(0)
}

/// Tauri command exposing today's bucket count to the panel UI. Lets the stats card
/// render "今天开口 X 次" alongside the lifetime total.
#[tauri::command]
pub async fn get_today_speech_count() -> u64 {
    today_speech_count().await
}

/// Iter 74: pure helper — sum the daily-bucket map across the last `n` days
/// ending at `today` (inclusive). Used by `week_speech_count` so the same
/// arithmetic is unit-testable without the on-disk daily file. `n=7` gives
/// "today + 6 prior days = rolling week".
pub fn sum_recent_days(map: &BTreeMap<String, u64>, today: chrono::NaiveDate, n: usize) -> u64 {
    let mut total: u64 = 0;
    for offset in 0..n {
        let d = today - chrono::Duration::days(offset as i64);
        let key = d.format("%Y-%m-%d").to_string();
        if let Some(v) = map.get(&key) {
            total = total.saturating_add(*v);
        }
    }
    total
}

/// Number of proactive utterances recorded across the trailing 7-day window
/// (today + 6 prior days, local time). Returns 0 if the daily file is missing.
pub async fn week_speech_count() -> u64 {
    let Some(path) = daily_path() else {
        return 0;
    };
    let content = tokio::fs::read_to_string(&path).await.unwrap_or_default();
    let map = parse_daily(&content);
    sum_recent_days(&map, chrono::Local::now().date_naive(), 7)
}

/// Tauri command for the panel — trailing 7-day proactive speech count.
#[tauri::command]
pub async fn get_week_speech_count() -> u64 {
    week_speech_count().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_recent_empty_content() {
        assert!(parse_recent("", 5).is_empty());
    }

    #[test]
    fn parse_recent_n_zero() {
        assert!(parse_recent("a\nb\nc\n", 0).is_empty());
    }

    #[test]
    fn parse_recent_fewer_than_n() {
        let v = parse_recent("a\nb\n", 5);
        assert_eq!(v, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn parse_recent_exactly_n() {
        let v = parse_recent("a\nb\nc\n", 3);
        assert_eq!(v, vec!["a".to_string(), "b".to_string(), "c".to_string()]);
    }

    #[test]
    fn parse_recent_more_than_n_keeps_tail() {
        let v = parse_recent("a\nb\nc\nd\ne\n", 3);
        assert_eq!(v, vec!["c".to_string(), "d".to_string(), "e".to_string()]);
    }

    #[test]
    fn parse_recent_skips_blank_lines() {
        let v = parse_recent("a\n\nb\n\nc\n", 5);
        assert_eq!(v, vec!["a".to_string(), "b".to_string(), "c".to_string()]);
    }

    #[test]
    fn strip_timestamp_normal_line() {
        let line = "2026-05-03T12:34:56+08:00 早上好啊";
        assert_eq!(strip_timestamp(line), "早上好啊");
    }

    #[test]
    fn strip_timestamp_no_space_returns_whole_line() {
        assert_eq!(strip_timestamp("noprefix"), "noprefix");
    }

    #[test]
    fn parse_daily_empty_or_malformed() {
        assert!(parse_daily("").is_empty());
        assert!(parse_daily("not json").is_empty());
        assert!(parse_daily("[1, 2, 3]").is_empty());
    }

    #[test]
    fn parse_daily_valid_object() {
        let m = parse_daily(r#"{"2026-05-01": 3, "2026-05-02": 5}"#);
        assert_eq!(m.get("2026-05-01"), Some(&3));
        assert_eq!(m.get("2026-05-02"), Some(&5));
        assert_eq!(m.len(), 2);
    }

    #[test]
    fn prune_daily_drops_entries_before_cutoff() {
        let mut m = BTreeMap::new();
        m.insert("2026-01-01".to_string(), 10);
        m.insert("2026-04-01".to_string(), 20);
        m.insert("2026-05-01".to_string(), 30);
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 3).unwrap();
        let pruned = prune_daily(m, today, 30);
        // cutoff = 2026-04-03; 2026-01-01 (< cutoff) drops, 2026-04-01 (< cutoff) drops,
        // 2026-05-01 (>= cutoff) stays.
        assert_eq!(pruned.len(), 1);
        assert!(pruned.contains_key("2026-05-01"));
    }

    #[test]
    fn prune_daily_keeps_unparseable_keys() {
        let mut m = BTreeMap::new();
        m.insert("not-a-date".to_string(), 7);
        m.insert("2026-01-01".to_string(), 1);
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 3).unwrap();
        let pruned = prune_daily(m, today, 30);
        assert!(pruned.contains_key("not-a-date"));
        assert!(!pruned.contains_key("2026-01-01"));
    }

    #[test]
    fn sum_recent_days_basic() {
        let mut m = BTreeMap::new();
        m.insert("2026-05-01".to_string(), 3);
        m.insert("2026-05-02".to_string(), 5);
        m.insert("2026-05-03".to_string(), 7); // today
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 3).unwrap();
        // 7-day window ending today = 2026-04-27..2026-05-03 inclusive. Only 3 days
        // present, others zero.
        assert_eq!(sum_recent_days(&m, today, 7), 3 + 5 + 7);
    }

    #[test]
    fn sum_recent_days_window_excludes_older() {
        let mut m = BTreeMap::new();
        m.insert("2026-04-26".to_string(), 100); // 7 days before today → excluded by 7-window
        m.insert("2026-05-03".to_string(), 4);
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 3).unwrap();
        // 7-day window: 2026-04-27..2026-05-03. 04-26 is outside.
        assert_eq!(sum_recent_days(&m, today, 7), 4);
    }

    #[test]
    fn sum_recent_days_zero_window_returns_zero() {
        let mut m = BTreeMap::new();
        m.insert("2026-05-03".to_string(), 99);
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 3).unwrap();
        assert_eq!(sum_recent_days(&m, today, 0), 0);
    }

    #[test]
    fn sum_recent_days_handles_empty_map() {
        let m = BTreeMap::new();
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 3).unwrap();
        assert_eq!(sum_recent_days(&m, today, 7), 0);
    }

    #[test]
    fn prune_daily_zero_retain_drops_everything_dated() {
        let mut m = BTreeMap::new();
        m.insert("2026-05-03".to_string(), 1);
        let today = chrono::NaiveDate::from_ymd_opt(2026, 5, 3).unwrap();
        let pruned = prune_daily(m, today, 0);
        // cutoff == today, "2026-05-03" >= "2026-05-03" → kept (today is always retained).
        assert!(pruned.contains_key("2026-05-03"));
    }

    fn fresh_temp_dir(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("pet-test-{}-{}", label, nanos));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Manual round-trip: write entries directly with the same trim semantics as
    /// record_speech_inner, then validate parse_recent reads back the tail. We don't go
    /// through record_speech_inner because that hard-codes the user's config_dir path;
    /// recreating the trim logic in tests keeps file IO opt-out.
    #[test]
    fn write_and_parse_round_trip_with_trim() {
        let dir = fresh_temp_dir("speech");
        let path = dir.join("speech_history.log");
        let mut entries: Vec<String> = (0..(SPEECH_HISTORY_CAP + 5))
            .map(|i| format!("2026-05-03T12:00:00+08:00 line {}", i))
            .collect();
        if entries.len() > SPEECH_HISTORY_CAP {
            let drop = entries.len() - SPEECH_HISTORY_CAP;
            entries.drain(0..drop);
        }
        std::fs::write(&path, entries.join("\n") + "\n").unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        let recent = parse_recent(&content, 3);
        assert_eq!(recent.len(), 3);
        // After trimming the first 5, lines 5..(50+5) remain; last 3 are 52, 53, 54.
        assert!(recent[2].ends_with("line 54"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    // -- Iter R11: detect_repeated_topic -------------------------------------

    fn line(text: &str) -> String {
        format!("2026-05-03T10:00:00+08:00 {}", text)
    }

    #[test]
    fn detect_repeated_topic_returns_none_for_empty_input() {
        assert!(detect_repeated_topic(&[], 4, 3).is_none());
    }

    #[test]
    fn detect_repeated_topic_returns_none_when_no_overlap() {
        let lines = vec![
            line("早上好今天怎么样"),
            line("最近天气不错适合散步"),
            line("中午吃了什么"),
        ];
        // No 4-char window appears in 3 distinct lines.
        assert!(detect_repeated_topic(&lines, 4, 3).is_none());
    }

    #[test]
    fn detect_repeated_topic_finds_chinese_topic_across_three_lines() {
        // "工作进展" appears in three lines → flagged.
        let lines = vec![
            line("看你在专心工作进展不错"),
            line("工作进展怎么样了"),
            line("聊聊你今天的工作进展吧"),
        ];
        let topic = detect_repeated_topic(&lines, 4, 3).expect("should detect");
        assert!(
            topic.contains("工作进展"),
            "expected to surface 工作进展, got '{}'",
            topic
        );
    }

    #[test]
    fn detect_repeated_topic_respects_min_distinct_lines() {
        // Only 2 lines share "周末出去" — below min_distinct_lines=3 → None.
        let lines = vec![
            line("周末出去走走吧"),
            line("周末出去吃饭怎么样"),
            line("今天天气不错"),
        ];
        assert!(detect_repeated_topic(&lines, 4, 3).is_none());
        // But min=2 → fires.
        assert!(detect_repeated_topic(&lines, 4, 2).is_some());
    }

    #[test]
    fn detect_repeated_topic_skips_whitespace_bearing_windows() {
        // "了 我" / " 我们" sliding across word boundary should not be flagged
        // even though it'd technically appear multiple times.
        let lines = vec![
            line("吃饭了 我们走"),
            line("回来了 我们一起"),
            line("睡觉了 我们再聊"),
        ];
        // Distinct words; only artifact "了 我" or " 我们" connects them across
        // whitespace — those are explicitly skipped.
        let topic = detect_repeated_topic(&lines, 4, 3);
        if let Some(t) = topic {
            assert!(
                !t.contains(' '),
                "topic should not contain whitespace, got '{}'",
                t
            );
        }
    }

    #[test]
    fn detect_repeated_topic_skips_uniform_char_windows() {
        // Test sentinel: "...." or "嗯嗯嗯嗯" are formatting/filler not topics.
        let lines = vec![
            line("嗯嗯嗯嗯继续吧"),
            line("好的嗯嗯嗯嗯"),
            line("嗯嗯嗯嗯让我想想"),
        ];
        let topic = detect_repeated_topic(&lines, 4, 3);
        // If anything fires it must NOT be the uniform-char window.
        if let Some(t) = topic {
            assert!(
                t != "嗯嗯嗯嗯",
                "uniform-char windows should be filtered, got '{}'",
                t
            );
        }
    }

    #[test]
    fn detect_repeated_topic_handles_short_lines() {
        // Lines shorter than ngram_size are silently skipped — no panic.
        let lines = vec![line("嗨"), line("好"), line("不错")];
        assert!(detect_repeated_topic(&lines, 4, 1).is_none());
    }

    // -- Iter R14: speeches_for_date -----------------------------------------

    fn ts_line(date: &str, time: &str, text: &str) -> String {
        // Format matches what record_speech writes: "YYYY-MM-DDTHH:MM:SS+TZ text".
        // Use a fixed offset (+08:00) so tests don't depend on the runner's tz.
        format!("{}T{}+08:00 {}", date, time, text)
    }

    fn nd(y: i32, m: u32, d: u32) -> chrono::NaiveDate {
        chrono::NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn speeches_for_date_empty_content_returns_empty() {
        assert!(speeches_for_date("", nd(2026, 5, 3), 5).is_empty());
    }

    #[test]
    fn speeches_for_date_zero_max_returns_empty() {
        let content = ts_line("2026-05-03", "10:00:00", "早上好");
        assert!(speeches_for_date(&content, nd(2026, 5, 3), 0).is_empty());
    }

    #[test]
    fn speeches_for_date_filters_by_date() {
        let content = [
            ts_line("2026-05-02", "23:00:00", "晚安"),
            ts_line("2026-05-03", "08:00:00", "早安"),
            ts_line("2026-05-03", "12:00:00", "中午好"),
            ts_line("2026-05-04", "08:00:00", "新一天"),
        ]
        .join("\n");
        let out = speeches_for_date(&content, nd(2026, 5, 3), 5);
        assert_eq!(out.len(), 2);
        assert!(out[0].contains("早安"));
        assert!(out[1].contains("中午好"));
    }

    #[test]
    fn speeches_for_date_returns_last_max_when_more_match() {
        let content = [
            ts_line("2026-05-03", "08:00:00", "a"),
            ts_line("2026-05-03", "10:00:00", "b"),
            ts_line("2026-05-03", "12:00:00", "c"),
            ts_line("2026-05-03", "14:00:00", "d"),
        ]
        .join("\n");
        let out = speeches_for_date(&content, nd(2026, 5, 3), 2);
        // Last 2 in chronological order: c, d.
        assert_eq!(out.len(), 2);
        assert!(out[0].ends_with(" c"));
        assert!(out[1].ends_with(" d"));
    }

    #[test]
    fn speeches_for_date_skips_malformed_lines() {
        // Garbage line + line without timestamp + valid line — only the
        // valid one passes the filter.
        let content = [
            "garbage no space".to_string(),
            "not-a-timestamp line".to_string(),
            ts_line("2026-05-03", "10:00:00", "早上好"),
        ]
        .join("\n");
        let out = speeches_for_date(&content, nd(2026, 5, 3), 5);
        assert_eq!(out.len(), 1);
        assert!(out[0].contains("早上好"));
    }

    #[test]
    fn speeches_for_date_target_date_with_no_matches_returns_empty() {
        let content = ts_line("2026-05-03", "10:00:00", "今天的话");
        // Looking for yesterday — no match.
        assert!(speeches_for_date(&content, nd(2026, 5, 2), 5).is_empty());
    }

    fn ts(text: &str) -> String {
        format!("2026-05-04T12:00:00+08:00 {}", text)
    }

    #[test]
    fn length_hint_returns_empty_below_min_samples() {
        // R19: less than 3 samples = empty (not enough signal).
        assert_eq!(format_speech_length_hint(&[]), "");
        assert_eq!(
            format_speech_length_hint(&[ts("早上好啊好朋友今天怎么样")]),
            ""
        );
        assert_eq!(
            format_speech_length_hint(&[
                ts("早上好啊好朋友今天怎么样"),
                ts("中午吃了吗最近忙不忙啊"),
            ]),
            ""
        );
    }

    #[test]
    fn length_hint_fires_when_all_long() {
        // 3 lines all ≥ 25 chars → "偏长" hint.
        let lines = vec![
            ts("今天打算把那个超长的项目报告好好处理一下再放松休息吃饭"), // 27
            ts("最近这几天看起来真忙的样子要不要停下来多喝几口水休息会儿"), // 28
            ts("昨晚那本小说终于读完了我一直好奇结尾的反转给我讲讲嘛朋友"), // 28
        ];
        let hint = format_speech_length_hint(&lines);
        assert!(hint.contains("偏长"), "got {}", hint);
        assert!(hint.contains("更短"));
    }

    #[test]
    fn length_hint_fires_when_all_short() {
        // 3 lines all ≤ 8 chars → "偏短" hint.
        let lines = vec![ts("嘿"), ts("在吗？"), ts("吃了吗？")];
        let hint = format_speech_length_hint(&lines);
        assert!(hint.contains("偏短"), "got {}", hint);
        assert!(hint.contains("多花两句"));
    }

    #[test]
    fn length_hint_returns_empty_for_mixed_register() {
        // Mixed: 1 short + 2 long → already varying, no nudge.
        let lines = vec![
            ts("嘿"),
            ts("今天打算把那个超长的项目报告好好处理一下再放松休息吃饭"),
            ts("昨晚那本小说终于读完了我一直好奇结尾的反转给我讲讲嘛朋友"),
        ];
        assert_eq!(format_speech_length_hint(&lines), "");
    }

    #[test]
    fn length_hint_handles_chinese_correctly() {
        // 30 chars 中文 should register as 30 (chars().count()), not 90 (bytes).
        let line_30_chars = "一二三四五六七八九十十一十二十三十四十五十六十七十八十九二十二十一二十二二十三二十四二十五";
        let lines = vec![ts(line_30_chars), ts(line_30_chars), ts(line_30_chars)];
        let hint = format_speech_length_hint(&lines);
        assert!(hint.contains("偏长"));
    }

    #[test]
    fn length_hint_skips_empty_lines() {
        // Empty stripped lines shouldn't drag mean to 0 / register as "短".
        let lines = vec![
            ts(""),
            ts("今天打算把那个超长的项目报告好好处理一下再放松休息吃饭"),
            ts("昨晚那本小说终于读完了我一直好奇结尾的反转给我讲讲嘛朋友"),
            ts("最近这几天看起来真忙的样子要不要停下来多喝几口水休息会儿"),
        ];
        // 1 empty + 3 long → 3 nonzero ≥ min_samples, all long → 偏长
        let hint = format_speech_length_hint(&lines);
        assert!(hint.contains("偏长"), "got {}", hint);
    }

    #[test]
    fn length_hint_returns_empty_when_too_few_nonzero() {
        // 4 lines but 2 are empty → only 2 nonzero, below threshold.
        let lines = vec![
            ts(""),
            ts(""),
            ts("今天打算把那个超长的项目报告好好处理一下再放松休息吃饭"),
            ts("最近这几天看起来真忙的样子要不要停下来多喝几口水休息会儿"),
        ];
        assert_eq!(format_speech_length_hint(&lines), "");
    }

    #[test]
    fn length_hint_includes_sample_count_and_mean() {
        let lines = vec![ts("嘿"), ts("好的"), ts("好啊")];
        let hint = format_speech_length_hint(&lines);
        // mean = (1 + 2 + 2) / 3 = 1
        assert!(hint.contains("3 句"));
        assert!(hint.contains("平均"));
    }

    #[test]
    fn classify_register_returns_none_below_min_samples() {
        assert!(classify_speech_register(&[]).is_none());
        assert!(classify_speech_register(&[ts("一"), ts("二")]).is_none());
    }

    #[test]
    fn classify_register_returns_long_when_all_long() {
        let lines = vec![
            ts("今天打算把那个超长的项目报告好好处理一下再放松休息吃饭"),
            ts("最近这几天看起来真忙的样子要不要停下来多喝几口水休息会儿"),
            ts("昨晚那本小说终于读完了我一直好奇结尾的反转给我讲讲嘛朋友"),
        ];
        let summary = classify_speech_register(&lines).unwrap();
        assert_eq!(summary.kind, "long");
        assert_eq!(summary.samples, 3);
        assert!(summary.mean_chars >= 25);
    }

    #[test]
    fn classify_register_returns_short_when_all_short() {
        let lines = vec![ts("嘿"), ts("好的"), ts("吃了吗？")];
        let summary = classify_speech_register(&lines).unwrap();
        assert_eq!(summary.kind, "short");
        assert!(summary.mean_chars <= 8);
    }

    #[test]
    fn classify_register_returns_mixed_for_varied_register() {
        // R20: "mixed" is now an explicit return value (not collapsed to None).
        // Panel needs to render "📏 混合" chip even when LLM gets no nudge.
        let lines = vec![
            ts("嘿"),
            ts("今天打算把那个超长的项目报告好好处理一下再放松休息吃饭"),
            ts("昨晚那本小说终于读完了我一直好奇结尾的反转给我讲讲嘛朋友"),
        ];
        let summary = classify_speech_register(&lines).unwrap();
        assert_eq!(summary.kind, "mixed");
    }
}
