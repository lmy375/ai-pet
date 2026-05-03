//! Detects "the system just woke up from sleep" without depending on macOS-specific
//! NSWorkspace notifications. Strategy: every iteration of the proactive spawn loop
//! records a heartbeat. If the wall-clock gap between two heartbeats is much larger than
//! the loop's normal sleep interval, the process was suspended — i.e. the user closed the
//! lid or unlocked after a long away. The proactive prompt can then drop in a "looks
//! like you just got back" hint instead of greeting like nothing happened.
//!
//! Cross-platform by construction. Threshold is conservative (5 min) so a brief network
//! hiccup or scheduler stall doesn't trigger false wakes.

use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::Mutex as TokioMutex;

/// How long between consecutive heartbeats before we consider it a wake event. Proactive
/// loop normally sleeps `interval_seconds` (default 300), so a value just above that
/// would false-trigger every second tick. 600 keeps real wakes (laptops sleep for tens
/// of minutes typically) while ignoring routine scheduling jitter.
const WAKE_GAP_THRESHOLD_SECS: u64 = 600;

struct WakeInner {
    last_observation: Option<Instant>,
    last_wake_at: Option<Instant>,
}

pub struct WakeDetector {
    inner: TokioMutex<WakeInner>,
}

impl WakeDetector {
    pub fn new() -> Self {
        Self {
            inner: TokioMutex::new(WakeInner {
                last_observation: None,
                last_wake_at: None,
            }),
        }
    }

    /// Record a heartbeat. Returns `Some(elapsed)` if the gap since the prior heartbeat
    /// crossed the threshold (i.e. a wake was likely), `None` for a normal tick or the
    /// very first observation.
    pub async fn observe(&self) -> Option<Duration> {
        let now = Instant::now();
        let mut g = self.inner.lock().await;
        let elapsed = g
            .last_observation
            .and_then(|prev| detect_wake(Some(prev), now, WAKE_GAP_THRESHOLD_SECS));
        g.last_observation = Some(now);
        if elapsed.is_some() {
            g.last_wake_at = Some(now);
        }
        elapsed
    }

    /// Seconds since the most recently detected wake event, or None if we've never seen
    /// one. Used by the proactive prompt to decide whether to add a "welcome back" hint.
    pub async fn last_wake_seconds_ago(&self) -> Option<u64> {
        self.inner
            .lock()
            .await
            .last_wake_at
            .map(|t| t.elapsed().as_secs())
    }
}

/// Pure detection function — given the previous observation time and now, returns
/// `Some(gap)` if the gap exceeds `threshold_secs`. Extracted so tests can pass
/// arbitrary Instants without sleeping.
pub fn detect_wake(prev: Option<Instant>, now: Instant, threshold_secs: u64) -> Option<Duration> {
    prev.and_then(|p| {
        let d = now.checked_duration_since(p)?;
        if d.as_secs() > threshold_secs {
            Some(d)
        } else {
            None
        }
    })
}

pub type WakeDetectorStore = Arc<WakeDetector>;

pub fn new_wake_detector() -> WakeDetectorStore {
    Arc::new(WakeDetector::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_observation_returns_none() {
        let now = Instant::now();
        assert!(detect_wake(None, now, 600).is_none());
    }

    #[test]
    fn small_gap_no_wake() {
        let now = Instant::now();
        let prev = now.checked_sub(Duration::from_secs(60)).unwrap();
        assert!(detect_wake(Some(prev), now, 600).is_none());
    }

    #[test]
    fn at_threshold_no_wake() {
        let now = Instant::now();
        let prev = now.checked_sub(Duration::from_secs(600)).unwrap();
        // > threshold (strict), exactly equal does not trigger.
        assert!(detect_wake(Some(prev), now, 600).is_none());
    }

    #[test]
    fn beyond_threshold_detects_wake() {
        let now = Instant::now();
        let prev = now.checked_sub(Duration::from_secs(900)).unwrap();
        let d = detect_wake(Some(prev), now, 600).expect("wake detected");
        assert!(d.as_secs() >= 900);
    }

    #[test]
    fn now_before_prev_returns_none() {
        // Clock skew safety — if some bug puts now before prev, don't pretend a wake.
        let now = Instant::now();
        let prev = now + Duration::from_secs(60);
        assert!(detect_wake(Some(prev), now, 600).is_none());
    }
}
