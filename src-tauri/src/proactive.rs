//! Proactive engagement engine.
//!
//! Spawns a background loop that wakes up periodically and decides whether the pet should
//! initiate a conversation with the user. Currently uses a single signal — time since the last
//! interaction — and asks the LLM whether to speak. Future iterations will add active-app
//! detection, idle-input detection, and mood state.
//!
//! Wire-up: see `lib.rs`. The engine is started once in `setup`. It writes proactive replies
//! into the active session and emits a `proactive-message` Tauri event the frontend listens for.

// Iter QG5 incremental: reminders subsystem extracted to a submodule. The
// glob `pub use` re-exports the public API so external callers
// (`consolidate.rs`, panel commands) keep reaching items via the historical
// `crate::proactive::ReminderTarget` / `parse_reminder_prefix` paths.
mod butler_schedule;
mod clock;
mod cooldown;
mod daily_review;
mod gate;
mod loop_runner;
mod manual_trigger;
mod morning_briefing;
mod os_notification;
mod prompt_assembler;
mod prompt_rules;
mod reminder_cluster;
pub mod reminder_context;
mod reminder_hints;
mod reminders;
mod session_helpers;
mod task_hints;
mod telemetry;
mod time_helpers;
mod tone_snapshot;
mod transient;
mod turn;
mod unread_tray;
mod welcome_back;
pub use self::butler_schedule::*;
pub use self::clock::*;
pub use self::cooldown::*;
pub use self::daily_review::*;
pub use self::gate::*;
pub use self::loop_runner::*;
pub use self::manual_trigger::*;
pub use self::morning_briefing::*;
pub use self::os_notification::*;
pub use self::prompt_assembler::*;
pub use self::prompt_rules::*;
pub use self::reminder_hints::*;
pub use self::reminders::*;
pub use self::task_hints::*;
pub use self::telemetry::*;
pub use self::time_helpers::*;
pub use self::tone_snapshot::*;
pub use self::transient::*;
pub use self::turn::*;
pub use self::unread_tray::clear as clear_unread_proactive;

// Iter QG5e: in-memory stashes (LAST_PROACTIVE_*, LAST_FEEDBACK_RECORDED_FOR,
// LAST_PROACTIVE_TURNS) + TurnRecord + ProactiveTurnMeta + Tauri commands
// (get_last_proactive_prompt / _reply / _meta / get_recent_proactive_turns)
// extracted to `proactive/telemetry.rs`.


// Iter QG5c2: SILENT_MARKER moved to `proactive/prompt_assembler.rs` as
// `pub const`. Re-exported via the glob above so `run_proactive_turn` (which
// checks `reply.contains(SILENT_MARKER)`) keeps the bare-name reference.

// Iter QG5d: LoopAction enum moved to `proactive/gate.rs` (re-exported via
// glob above). Spawn-loop body below consumes it via the bare-name path.

// Iter QG5c1: ENV_AWARENESS_*, LONG_IDLE_MINUTES, LONG_ABSENCE_MINUTES
// extracted to `proactive/prompt_rules.rs` (re-exported via glob above).

// Iter QG5c1: companionship_milestone moved to `proactive/prompt_rules.rs`
// (it's the rule-label producer for the `companionship-milestone` data-driven
// rule). format_companionship_line above stays — that's the prompt-line
// renderer, not a rule label.


// Iter QG5c-prep: pure time/calendar/idle-band helpers (idle_tier /
// user_absence_tier / period_of_day / weekday_zh / weekday_kind_zh /
// format_day_of_week_hint / minutes_until_quiet_start / in_quiet_hours)
// extracted to `proactive/time_helpers.rs`. Re-exported via the glob at
// the top of this file.

// Iter QG5d: gate logic (LoopAction, evaluate_pre_input_idle,
// evaluate_input_idle_gate, evaluate_loop_tick, wake_recent,
// WAKE_GRACE_WINDOW_SECS) moved to `proactive/gate.rs`.






#[cfg(test)]
mod prompt_tests;
