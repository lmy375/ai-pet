//! CGEvent input: key table, modifier parsing, and postToPid delivery.
//!
//! Everything posts to the target pid only — the user's real cursor and
//! keyboard are never touched. Caveat (measured on macOS 26): not every app
//! accepts pid-scoped keyboard events (Calculator ignores them outright), so
//! the library routes typing through AXValue writes first and only falls back
//! to events.

use core_graphics::event::{CGEvent, CGEventFlags, CGEventType, KeyCode};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use core_graphics_types::geometry::CGPoint;
use foreign_types::ForeignType;

use crate::AxError;

// core-graphics 0.25 doesn't wrap this: without the unicode string payload
// many apps see an empty `characters` field and drop the event entirely.
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventKeyboardSetUnicodeString(
        event: core_graphics::sys::CGEventRef,
        length: isize,
        string: *const u16,
    );
}

fn set_unicode(event: &CGEvent, text: &str) {
    let units: Vec<u16> = text.encode_utf16().collect();
    unsafe { CGEventKeyboardSetUnicodeString(event.as_ptr(), units.len() as isize, units.as_ptr()) };
}

fn event_source() -> CGEventSource {
    // HIDSystemState makes the system treat these as real hardware input
    // (modifier flags tracked properly) — CombinedSessionState events are
    // often dropped by AppKit apps.
    CGEventSource::new(CGEventSourceStateID::HIDSystemState).expect("event source")
}

/// Parse a key or combo like "enter", "cmd+c", "ctrl+shift+tab" into a
/// keycode plus modifier flags.
pub fn parse_key(combo: &str) -> Result<(u16, CGEventFlags), AxError> {
    let mut flags = CGEventFlags::empty();
    let mut code = None;
    for part in combo.split('+') {
        match part.to_ascii_lowercase().as_str() {
            "cmd" | "command" | "meta" => flags |= CGEventFlags::CGEventFlagCommand,
            "ctrl" | "control" => flags |= CGEventFlags::CGEventFlagControl,
            "alt" | "option" | "opt" => flags |= CGEventFlags::CGEventFlagAlternate,
            "shift" => flags |= CGEventFlags::CGEventFlagShift,
            "fn" => flags |= CGEventFlags::CGEventFlagSecondaryFn,
            name => {
                if code.is_some() {
                    return Err(AxError::BadKey(combo.into()));
                }
                code = Some(
                    key_code(name).ok_or_else(|| AxError::BadKey(format!("{combo}: unknown key '{name}'")))?,
                );
            }
        }
    }
    code.map(|c| (c, flags)).ok_or_else(|| AxError::BadKey(combo.into()))
}

/// Named key → virtual keycode. Single ascii letters/digits map to their key
/// position (ANSI layout); the keypad variants are spelled out explicitly.
fn key_code(name: &str) -> Option<u16> {
    let code = match name {
        "enter" | "return" => KeyCode::RETURN,
        "tab" => KeyCode::TAB,
        "space" => KeyCode::SPACE,
        "delete" | "backspace" => KeyCode::DELETE,
        "esc" | "escape" => KeyCode::ESCAPE,
        "up" | "arrowup" => KeyCode::UP_ARROW,
        "down" | "arrowdown" => KeyCode::DOWN_ARROW,
        "left" | "arrowleft" => KeyCode::LEFT_ARROW,
        "right" | "arrowright" => KeyCode::RIGHT_ARROW,
        "home" => KeyCode::HOME,
        "end" => KeyCode::END,
        "pageup" => KeyCode::PAGE_UP,
        "pagedown" => KeyCode::PAGE_DOWN,
        "f1" => KeyCode::F1,
        "f2" => KeyCode::F2,
        "f3" => KeyCode::F3,
        "f4" => KeyCode::F4,
        "f5" => KeyCode::F5,
        "f6" => KeyCode::F6,
        "f7" => KeyCode::F7,
        "f8" => KeyCode::F8,
        "f9" => KeyCode::F9,
        "f10" => KeyCode::F10,
        "f11" => KeyCode::F11,
        "f12" => KeyCode::F12,
        c if c.chars().count() == 1 => {
            // Apple's ANSI keycodes follow QWERTY key position, not the
            // alphabet — an explicit table is the only correct mapping.
            let ch = c.chars().next().expect("one char").to_ascii_lowercase();
            match ch {
                'a' => KeyCode::ANSI_A,
                'b' => KeyCode::ANSI_B,
                'c' => KeyCode::ANSI_C,
                'd' => KeyCode::ANSI_D,
                'e' => KeyCode::ANSI_E,
                'f' => KeyCode::ANSI_F,
                'g' => KeyCode::ANSI_G,
                'h' => KeyCode::ANSI_H,
                'i' => KeyCode::ANSI_I,
                'j' => KeyCode::ANSI_J,
                'k' => KeyCode::ANSI_K,
                'l' => KeyCode::ANSI_L,
                'm' => KeyCode::ANSI_M,
                'n' => KeyCode::ANSI_N,
                'o' => KeyCode::ANSI_O,
                'p' => KeyCode::ANSI_P,
                'q' => KeyCode::ANSI_Q,
                'r' => KeyCode::ANSI_R,
                's' => KeyCode::ANSI_S,
                't' => KeyCode::ANSI_T,
                'u' => KeyCode::ANSI_U,
                'v' => KeyCode::ANSI_V,
                'w' => KeyCode::ANSI_W,
                'x' => KeyCode::ANSI_X,
                'y' => KeyCode::ANSI_Y,
                'z' => KeyCode::ANSI_Z,
                '0' => KeyCode::ANSI_0,
                '1' => KeyCode::ANSI_1,
                '2' => KeyCode::ANSI_2,
                '3' => KeyCode::ANSI_3,
                '4' => KeyCode::ANSI_4,
                '5' => KeyCode::ANSI_5,
                '6' => KeyCode::ANSI_6,
                '7' => KeyCode::ANSI_7,
                '8' => KeyCode::ANSI_8,
                '9' => KeyCode::ANSI_9,
                _ => return None,
            }
        }
        _ => return None,
    };
    Some(code)
}

/// Post one keypress (down + up) with modifiers to a pid.
pub fn post_key(pid: i32, combo: &str) -> Result<(), AxError> {
    let (code, flags) = parse_key(combo)?;
    let source = event_source();
    for down in [true, false] {
        let event = CGEvent::new_keyboard_event(source.clone(), code, down)
            .map_err(|_| AxError::Ax("failed to create keyboard event".into()))?;
        if !flags.is_empty() {
            event.set_flags(flags);
        }
        event.post_to_pid(pid);
    }
    Ok(())
}

/// Post a text string as per-character keyboard events to a pid. The unicode
/// payload carries the actual character so non-ASCII survives.
pub fn post_text(pid: i32, text: &str) {
    let source = event_source();
    for c in text.chars() {
        // Best-effort keycode so apps that read keys see a plausible one;
        // the unicode string is what most apps actually consume.
        let code = key_code(&c.to_lowercase().to_string()).unwrap_or(KeyCode::RETURN);
        for down in [true, false] {
            let event = CGEvent::new_keyboard_event(source.clone(), code, down)
                .expect("keyboard event");
            set_unicode(&event, &c.to_string());
            event.post_to_pid(pid);
        }
    }
}

/// Post a left click (down + up) at a global-screen point to a pid.
pub fn post_click(pid: i32, point: CGPoint) {
    let source = event_source();
    for event_type in [CGEventType::LeftMouseDown, CGEventType::LeftMouseUp] {
        let event = CGEvent::new_mouse_event(
            source.clone(),
            event_type,
            point,
            core_graphics::event::CGMouseButton::Left,
        )
        .expect("mouse event");
        event.post_to_pid(pid);
    }
}

/// Post a scroll-wheel event to a pid. AX actions are preferred by callers;
/// this is the fallback for elements without scroll actions.
pub fn post_scroll(pid: i32, up: bool) {
    use core_graphics::event::ScrollEventUnit;
    let source = event_source();
    // 3 lines ≈ one notch of a real wheel.
    let delta: i32 = if up { 3 } else { -3 };
    if let Ok(event) = CGEvent::new_scroll_event(
        source,
        ScrollEventUnit::LINE,
        1,
        delta,
        0,
        0,
    ) {
        event.post_to_pid(pid);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_keys_parse_without_flags() {
        let (code, flags) = parse_key("enter").unwrap();
        assert_eq!(code, KeyCode::RETURN);
        assert!(flags.is_empty());
    }

    #[test]
    fn combos_stack_modifiers_left_to_right() {
        let (code, flags) = parse_key("cmd+shift+t").unwrap();
        assert_eq!(code, KeyCode::ANSI_T);
        assert!(flags.contains(CGEventFlags::CGEventFlagCommand));
        assert!(flags.contains(CGEventFlags::CGEventFlagShift));
    }

    #[test]
    fn aliases_map_to_the_same_code() {
        assert_eq!(key_code("esc"), key_code("escape"));
        assert_eq!(key_code("return"), key_code("enter"));
    }

    #[test]
    fn unknown_keys_are_rejected() {
        assert!(parse_key("notakey").is_err());
        assert!(parse_key("cmd++").is_err());
        // Two non-modifier parts is a combo typo, not a key.
        assert!(parse_key("a+b").is_err());
    }
}
