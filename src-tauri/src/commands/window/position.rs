//! Position persistence for the pet and ball windows.
//!
//! Saved coordinates are **logical** pixels (rounded to integers when written),
//! the same space window positions are read back in, so a 2x display restores
//! the window where the user left it.

use pet_core::settings::{get_settings, set_ball_position, set_window_position, WindowPosition};
use tauri::{AppHandle, Manager, WebviewWindow};

use super::geometry::{clamp_position, fit_position, fit_size, monitor_work_areas};

/// Keep the pet window inside a display. This only moves the window: the
/// frontend calls it from move and resize handlers, and a resize here would
/// feed the resize event back into those handlers.
#[tauri::command]
pub fn keep_window_visible(window: WebviewWindow) -> Result<(), String> {
    clamp_position(&window).map(|_| ())
}

/// Persist the pet window's top-left position so it reopens where the user left
/// it. Called (debounced) from the frontend whenever the user moves the window.
#[tauri::command]
pub fn save_window_position(window: WebviewWindow) -> Result<(), String> {
    let pos = fit_size(&window)?;
    set_window_position(pos.x.round() as i32, pos.y.round() as i32)
}

fn load_window_position() -> Option<WindowPosition> {
    get_settings().ok()?.window
}

/// Saved floating-ball position (see `save_ball_position`).
pub(crate) fn load_ball_position() -> Option<WindowPosition> {
    get_settings().ok()?.ball
}

/// Persist the ball's top-left position so it reappears where the user dragged
/// it. Called (debounced) from the ball webview after a drag.
#[tauri::command]
pub fn save_ball_position(window: WebviewWindow) -> Result<(), String> {
    let pos = fit_size(&window)?;
    set_ball_position(pos.x.round() as i32, pos.y.round() as i32)
}

/// Restore the saved pet-window position onto a connected display, clamping it
/// so the window stays visible; center it only when no work area is available.
/// The saved position is logical pixels, so it is used as-is here.
///
/// This always shows the pet. Callers that can run while the panel is open must
/// use `present_foreground` instead.
pub fn restore_main_window(app: &AppHandle) {
    let win = match app.get_webview_window("main") {
        Some(w) => w,
        None => return,
    };
    let restored = load_window_position().and_then(|pos| {
        let scale = win.scale_factor().ok()?;
        let size = win.outer_size().ok()?.to_logical::<f64>(scale);
        let areas = monitor_work_areas(&win).ok()?;
        fit_position(pos.x as f64, pos.y as f64, size.width, size.height, &areas)
    });
    match restored {
        Some(pos) => {
            let _ = win.set_position(pos);
        }
        None => {
            let _ = win.center();
        }
    }
    let _ = win.show();
    let _ = win.set_focus();
}
