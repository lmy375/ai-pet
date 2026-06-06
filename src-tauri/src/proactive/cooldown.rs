//! Cooldown breakdown derivation + per-speech meta-recording wrapper.
//!
//! `build_cooldown_breakdown` mirrors the gate's effective-cooldown math
//! (configured × companion_mode × R7-feedback × R81-deadline) so the panel
//! chip hover renders the same numbers the gate is actually enforcing.
//!
//! `record_speech_with_current_meta` is the speech-history write wrapper used
//! by every proactive emit path (run_proactive_turn / morning_briefing /
//! welcome_back) — it samples the same cooldown breakdown so each speech
//! gets persisted alongside the "why did the pet open mouth" trigger meta.

use crate::commands::settings::get_settings;

use super::butler_schedule::{
    count_urgent_butler_deadlines, deadline_urgency_factor, parse_butler_deadline_prefix,
};
use super::tone_snapshot::CooldownBreakdown;

/// Iter #389: 写 speech 同时附 per-speech 触发 meta 进 sidecar JSONL —
/// 让 PanelDebug ⏰ chip "为何开口" 半边能 audit。meta 来自 build_cooldown_
/// breakdown 同算法，与 ToneStrip 当前态 chip 一致。proactive disabled
/// （build_cooldown_breakdown 返 None）兜底 meta 用 "insufficient_samples"
/// / 1.0 — 罕见场景（speech 写时 proactive 一定 enabled），仅 defensive。
pub(super) async fn record_speech_with_current_meta(text: &str) {
    let recent_fb = crate::feedback_history::recent_feedback(20).await;
    let urgent_count = {
        let now = chrono::Local::now().naive_local();
        let items: Vec<(chrono::NaiveDateTime, String)> =
            crate::db::butler_tasks_as_memory_items()
                .iter()
                .filter_map(|i| parse_butler_deadline_prefix(&i.description))
                .collect();
        count_urgent_butler_deadlines(&items, now)
    };
    let meta = match build_cooldown_breakdown(&recent_fb, urgent_count) {
        Some(b) => crate::speech_history::SpeechMeta {
            ts: String::new(),
            band: b.feedback_band,
            factor: b.feedback_factor,
            mode: b.mode,
            deadline_factor: b.deadline_factor,
        },
        None => crate::speech_history::SpeechMeta {
            ts: String::new(),
            band: "insufficient_samples".to_string(),
            factor: 1.0,
            mode: String::new(),
            deadline_factor: 1.0,
        },
    };
    crate::speech_history::record_speech_with_meta(text, meta).await;
}

/// Iter R23: derive the cooldown breakdown for the panel chip hover.
/// Mirrors `gate.rs`'s effective-cooldown computation exactly so the
/// chip's "configured × mode × feedback × deadline = effective" math
/// matches the number the gate is actually enforcing. Returns `None`
/// when proactive is disabled or configured cooldown is 0 (gate
/// effectively off in either case — no breakdown to show).
///
/// Iter R81: `urgent_deadline_count` (Imminent + Overdue butler tasks)
/// drives a discrete 0.5× shrink on top of the R7 feedback factor.
pub fn build_cooldown_breakdown(
    recent_fb: &[crate::feedback_history::FeedbackEntry],
    urgent_deadline_count: u64,
) -> Option<CooldownBreakdown> {
    let settings = get_settings().ok()?;
    if !settings.proactive.enabled {
        return None;
    }
    let configured = settings.proactive.cooldown_seconds;
    if configured == 0 {
        return None;
    }
    let mode = settings.proactive.companion_mode.clone();
    let after_mode = settings.proactive.effective_cooldown_base();
    // mode_factor: derive from the ratio so a future mode addition
    // (e.g. "ultra-quiet") shows up correctly without needing a hardcoded
    // table here.
    let mode_factor = if configured == 0 {
        1.0
    } else {
        after_mode as f64 / configured as f64
    };
    // Match feedback_history::adapted_cooldown_seconds branching exactly.
    // Pure helper in feedback_history isolates the band classification so
    // it can be unit-tested without get_settings() / Tauri state.
    let (feedback_band, feedback_factor) =
        crate::feedback_history::classify_feedback_band(recent_fb);
    let deadline_factor = deadline_urgency_factor(urgent_deadline_count);
    let effective = ((after_mode as f64) * feedback_factor * deadline_factor) as u64;
    Some(CooldownBreakdown {
        configured_seconds: configured,
        mode,
        mode_factor,
        after_mode_seconds: after_mode,
        feedback_band: feedback_band.to_string(),
        feedback_factor,
        urgent_deadline_count,
        deadline_factor,
        effective_seconds: effective,
    })
}

#[cfg(test)]
mod cooldown_breakdown_tests {
    use crate::feedback_history::{classify_feedback_band, FeedbackEntry, FeedbackKind};

    fn entry(kind: FeedbackKind) -> FeedbackEntry {
        FeedbackEntry {
            timestamp: "2026-05-04T12:00:00+08:00".to_string(),
            kind,
            excerpt: "x".to_string(),
        }
    }

    #[test]
    fn band_insufficient_below_min_samples() {
        // R23: < 5 samples → "insufficient_samples", 1.0× (R7 returns base unchanged).
        let (band, factor) = classify_feedback_band(&[]);
        assert_eq!(band, "insufficient_samples");
        assert_eq!(factor, 1.0);
        let entries: Vec<_> = (0..4).map(|_| entry(FeedbackKind::Ignored)).collect();
        let (band, factor) = classify_feedback_band(&entries);
        assert_eq!(band, "insufficient_samples");
        assert_eq!(factor, 1.0);
    }

    #[test]
    fn band_high_negative_doubles() {
        // > 0.6 ratio: 4/5 ignored = 0.8 → high_negative, 2.0×.
        let mut entries = vec![entry(FeedbackKind::Ignored); 4];
        entries.push(entry(FeedbackKind::Replied));
        let (band, factor) = classify_feedback_band(&entries);
        assert_eq!(band, "high_negative");
        assert_eq!(factor, 2.0);
    }

    #[test]
    fn band_low_negative_shrinks() {
        // < 0.2 ratio: 1/10 ignored = 0.1 → low_negative, 0.7×.
        let mut entries = vec![entry(FeedbackKind::Ignored)];
        entries.extend(vec![entry(FeedbackKind::Replied); 9]);
        let (band, factor) = classify_feedback_band(&entries);
        assert_eq!(band, "low_negative");
        assert_eq!(factor, 0.7);
    }

    #[test]
    fn band_mid_keeps_base() {
        // 0.2 ≤ ratio ≤ 0.6 → "mid", 1.0× (cooldown unchanged).
        let mut entries = vec![entry(FeedbackKind::Ignored); 2]; // 2/5 = 0.4
        entries.extend(vec![entry(FeedbackKind::Replied); 3]);
        let (band, factor) = classify_feedback_band(&entries);
        assert_eq!(band, "mid");
        assert_eq!(factor, 1.0);
    }

    #[test]
    fn band_dismissed_counted_alongside_ignored() {
        // R1c: dismissed counts as negative. 3 dismissed + 2 replied = 0.6 ratio,
        // 0.6 is NOT > 0.6 (strict inequality) → mid band.
        let mut entries = vec![entry(FeedbackKind::Dismissed); 3];
        entries.extend(vec![entry(FeedbackKind::Replied); 2]);
        let (band, factor) = classify_feedback_band(&entries);
        assert_eq!(band, "mid");
        assert_eq!(factor, 1.0);
        // 4 dismissed + 1 replied = 0.8 → high_negative.
        let mut entries = vec![entry(FeedbackKind::Dismissed); 4];
        entries.push(entry(FeedbackKind::Replied));
        let (band, factor) = classify_feedback_band(&entries);
        assert_eq!(band, "high_negative");
        assert_eq!(factor, 2.0);
    }
}
