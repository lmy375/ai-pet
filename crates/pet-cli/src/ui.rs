//! Small text helpers + the ANSI codes still used by `-p` (one-shot) output.
//! The interactive TUI styles everything through ratatui instead.

pub const RESET: &str = "\x1b[0m";
pub const DIM: &str = "\x1b[2m";
pub const RED: &str = "\x1b[31m";
pub const YELLOW: &str = "\x1b[33m";

/// First line of `s`, truncated to `max` chars (with an ellipsis when cut) —
/// for compact one-line tool call/result display.
pub fn one_line(s: &str, max: usize) -> String {
    let first = s.lines().next().unwrap_or("");
    let truncated_line = s.lines().count() > 1;
    let t: String = first.chars().take(max).collect();
    if first.chars().count() > max || truncated_line {
        format!("{}…", t)
    } else {
        t
    }
}
