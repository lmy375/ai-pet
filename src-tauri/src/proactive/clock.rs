//! Interaction clock + companionship line: tracks last-interaction / last-
//! proactive timestamps + the "awaiting user reply" gate. Used by the gate
//! check, panel debug, and prompt_assembler companionship line.

use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex as TokioMutex;

/// Tracks interaction timing for the proactive engagement loop. Holds the last interaction
/// time, the last proactive utterance time, and whether the most recent proactive message
/// is still waiting for a user reply.
///
/// State transitions:
/// - `mark_user_message()` — user sent something. Clears `awaiting_user_reply`.
/// - `mark_proactive_spoken()` — pet spoke proactively. Sets `awaiting_user_reply = true`
///   and stamps `last_proactive`.
/// - `touch()` — any other interaction (e.g. assistant finished a reactive reply). Only
///   updates `last`; does not affect `awaiting_user_reply` or `last_proactive`.
pub struct InteractionClock {
    inner: TokioMutex<ClockInner>,
}

struct ClockInner {
    last: Instant,
    last_proactive: Option<Instant>,
    awaiting_user_reply: bool,
}

/// Snapshot of clock state used by the proactive scheduler to decide whether to fire.
pub struct ClockSnapshot {
    pub idle_seconds: u64,
    pub since_last_proactive_seconds: Option<u64>,
    pub awaiting_user_reply: bool,
}

impl InteractionClock {
    pub fn new() -> Self {
        Self {
            inner: TokioMutex::new(ClockInner {
                last: Instant::now(),
                last_proactive: None,
                awaiting_user_reply: false,
            }),
        }
    }

    pub async fn touch(&self) {
        let mut g = self.inner.lock().await;
        g.last = Instant::now();
    }

    /// Called when the user sends a message. Clears the awaiting-reply flag — once the user
    /// has spoken, we no longer consider any prior proactive message "ignored".
    pub async fn mark_user_message(&self) {
        let mut g = self.inner.lock().await;
        g.last = Instant::now();
        g.awaiting_user_reply = false;
    }

    /// Called after a proactive utterance is delivered. Sets `awaiting_user_reply = true`
    /// and records the time so cooldown checks can run.
    pub async fn mark_proactive_spoken(&self) {
        let now = Instant::now();
        let mut g = self.inner.lock().await;
        g.last = now;
        g.last_proactive = Some(now);
        g.awaiting_user_reply = true;
    }

    /// Iter R1: return the un-expired `awaiting_user_reply` flag without applying
    /// the D11 4-hour auto-clear. Used by feedback classification — for that
    /// purpose "user actually replied" is binary regardless of how long they
    /// took, while the *gate* check uses `effective_awaiting` so the pet doesn't
    /// stay muted across multi-day absences.
    pub async fn raw_awaiting(&self) -> bool {
        self.inner.lock().await.awaiting_user_reply
    }

    pub async fn snapshot(&self) -> ClockSnapshot {
        let g = self.inner.lock().await;
        let since_proactive = g.last_proactive.map(|t| t.elapsed().as_secs());
        ClockSnapshot {
            idle_seconds: g.last.elapsed().as_secs(),
            since_last_proactive_seconds: since_proactive,
            // Iter D11: auto-expire awaiting after AWAITING_AUTO_CLEAR_SECONDS.
            // The raw state in ClockInner stays — only `mark_user_message` is the
            // canonical clearer — but snapshot reports the effective gate state
            // so both proactive's gate check and the panel chip honor the same
            // expiry. Without this, a pet that spoke once and the user closed
            // the laptop without replying would stay muted indefinitely on the
            // next session, even hours later.
            awaiting_user_reply: effective_awaiting(g.awaiting_user_reply, since_proactive),
        }
    }
}

/// Iter D11: pure decider — given raw awaiting state and seconds since pet's last
/// proactive utterance, return whether the awaiting gate should still fire.
/// Returns true only when the raw flag is set AND the gap is short enough that
/// the original "polite-wait, don't double up" intent still applies. After
/// AWAITING_AUTO_CLEAR_SECONDS the gate falls off so the pet doesn't stay
/// permanently muted across long absences.
pub fn effective_awaiting(raw_awaiting: bool, since_last_proactive_seconds: Option<u64>) -> bool {
    if !raw_awaiting {
        return false;
    }
    match since_last_proactive_seconds {
        Some(secs) => secs < AWAITING_AUTO_CLEAR_SECONDS,
        // No prior proactive recorded but flag is set — treat as not-fresh.
        // Shouldn't really happen since mark_proactive_spoken sets both, but
        // belt-and-suspenders.
        None => false,
    }
}

/// Iter D11: how long the awaiting gate honors the raw flag before auto-clearing.
/// 4 hours: covers a typical "stepped away for lunch + meeting" without forcing
/// the user to chat back; long enough that re-firing the gate later isn't rude.
pub const AWAITING_AUTO_CLEAR_SECONDS: u64 = 4 * 3600;

pub type InteractionClockStore = Arc<InteractionClock>;

pub fn new_interaction_clock() -> InteractionClockStore {
    Arc::new(InteractionClock::new())
}

/// Render the "companionship duration" section of the proactive prompt. Day 0 gets a
/// "this is the first day" framing — encouraging the LLM to use a getting-acquainted
/// register — while N >= 1 just states the count, letting the LLM choose whether and
/// how to draw on the accumulated familiarity. Pure / testable.
pub fn format_companionship_line(days: u64) -> String {
    if days == 0 {
        "你和用户今天才正式认识，是你陪伴 ta 的第一天——语气可以保留一点点初识的客气感。".to_string()
    } else {
        format!(
            "你和用户已经一起走过 {} 天——可以让这份相处时长自然渗进语气，比如对 ta 偏好的预判、共同回忆的暗指（不必硬塞，时机对就用）。",
            days
        )
    }
}
