//! "How many days have we been together" — first-step of the long-term persona
//! evolution path (Iter 101 / route A in STATUS.md).
//!
//! Persists `install_date.txt` once on first run, then computes days-since-install
//! on demand. The proactive prompt picks that number up so the pet knows whether
//! it's been with the user for 1 day or 200, and can tune its register accordingly
//! (LLM does the actual phrasing — backend just exposes the number).

use std::path::PathBuf;

use chrono::NaiveDate;

fn install_date_path() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("pet").join("install_date.txt"))
}

/// Read the install_date.txt sidecar; on missing / malformed file, write today's date
/// and return it. Best-effort — IO failures fall back to today (in-memory only) so a
/// hosed disk doesn't block the proactive loop. The fallback means companionship_days
/// would temporarily be 0 in those edge cases, which is harmless.
pub async fn ensure_install_date() -> NaiveDate {
    let today = chrono::Local::now().date_naive();
    let Some(path) = install_date_path() else {
        return today;
    };
    if let Ok(s) = tokio::fs::read_to_string(&path).await {
        if let Some(parsed) = parse_install_date(&s) {
            return parsed;
        }
    }
    if let Some(parent) = path.parent() {
        let _ = tokio::fs::create_dir_all(parent).await;
    }
    let _ = tokio::fs::write(&path, format!("{}\n", today.format("%Y-%m-%d"))).await;
    today
}

/// Pure: extract a YYYY-MM-DD date from the file contents. Whitespace + trailing
/// junk tolerated (so a manually-edited file with a comment still works); anything
/// unparseable returns None and the caller treats it as "missing".
pub fn parse_install_date(content: &str) -> Option<NaiveDate> {
    let first = content.lines().next().unwrap_or("").trim();
    NaiveDate::parse_from_str(first, "%Y-%m-%d").ok()
}

/// Pure: how many days have elapsed from `install` to `today`. Negative differences
/// (clock skew, manually edited future date) collapse to 0 — the pet shouldn't ever
/// see a negative companionship duration.
pub fn days_between(install: NaiveDate, today: NaiveDate) -> u64 {
    let diff = (today - install).num_days();
    if diff < 0 {
        0
    } else {
        diff as u64
    }
}

/// Days since first install. 0 on the install day itself.
pub async fn companionship_days() -> u64 {
    let install = ensure_install_date().await;
    let today = chrono::Local::now().date_naive();
    days_between(install, today)
}

/// Tauri command exposing companionship days to the panel UI (Iter 106). Lets the
/// stats card show "陪伴 N 天" alongside today's and lifetime utterance counts.
/// Uses `ensure_install_date` so the first time the panel polls also bootstraps
/// the install_date.txt file — no need to wait for the first proactive turn.
#[tauri::command]
pub async fn get_companionship_days() -> u64 {
    companionship_days().await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_install_date_valid() {
        assert_eq!(
            parse_install_date("2026-01-15\n"),
            Some(NaiveDate::from_ymd_opt(2026, 1, 15).unwrap()),
        );
    }

    #[test]
    fn parse_install_date_with_trailing_comment() {
        // Only the first line is parsed; later content is ignored. Lets users add a
        // human-readable note below the date by hand.
        assert_eq!(
            parse_install_date("2025-11-30\n# pet was set up today on the new mac\n"),
            Some(NaiveDate::from_ymd_opt(2025, 11, 30).unwrap()),
        );
    }

    #[test]
    fn parse_install_date_malformed_returns_none() {
        assert!(parse_install_date("").is_none());
        assert!(parse_install_date("not a date").is_none());
        assert!(parse_install_date("2026/01/15").is_none());
        // Future-proofing: rejecting bogus values forces ensure_install_date to rewrite
        // the file with a fresh today, so a corrupted file self-heals.
    }

    #[test]
    fn days_between_zero_on_install_day() {
        let d = NaiveDate::from_ymd_opt(2026, 5, 3).unwrap();
        assert_eq!(days_between(d, d), 0);
    }

    #[test]
    fn days_between_counts_forward() {
        let install = NaiveDate::from_ymd_opt(2026, 5, 1).unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 5, 8).unwrap();
        assert_eq!(days_between(install, today), 7);
    }

    #[test]
    fn days_between_clamps_negative_to_zero() {
        // Install date in the future relative to "today" → clock skew or manual edit.
        // Should not underflow / produce nonsense; pet sees 0.
        let install = NaiveDate::from_ymd_opt(2030, 1, 1).unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 5, 3).unwrap();
        assert_eq!(days_between(install, today), 0);
    }
}
