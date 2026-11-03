//! Window surfaces: focus routing, the hide/show policy shared by the pet,
//! panel and ball, and the panel/debug windows.

use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

use super::geometry::clamp_position;
use super::position::restore_main_window;

// --- Active-window tracking (for routing background-task notifications) ---

/// The window label ("main" or "panel") that most recently gained focus. The pet
/// and panel share one conversation, so a background-task completion must be
/// injected into exactly one window — the one the user is actually looking at.
pub struct ActiveWindow(pub Mutex<String>);

/// Called by each window's focus handler so the backend knows where to route
/// completion notifications.
#[tauri::command]
pub fn set_active_window(label: String, state: tauri::State<'_, ActiveWindow>) {
    *state.0.lock().unwrap() = label;
}

/// The label to emit window-targeted events to: the active window, or "main" as a
/// fallback if that window no longer exists (e.g. the panel was closed).
pub fn active_window_label(app: &AppHandle) -> String {
    let label = app.state::<ActiveWindow>().0.lock().unwrap().clone();
    if app.get_webview_window(&label).is_some() {
        label
    } else {
        "main".to_string()
    }
}

/// Hide the pet and the floating ball, and tell the pet webview to tear down
/// Live2D. A WebGL context cannot survive the window being hidden.
fn hide_pet_surface(app: &AppHandle) {
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.hide();
    }
    if let Some(ball) = app.get_webview_window("ball") {
        let _ = ball.hide();
    }
    let _ = app.emit("main-hidden", ());
}

/// Bring the surface the user should be looking at to the front.
///
/// The panel replaces the pet for as long as that window exists — open,
/// minimized, or briefly hidden. Dock clicks and ball clicks both come through
/// here, so neither can show the pet beside an open panel. The pet is restored
/// only after the panel is destroyed.
pub fn present_foreground(app: &AppHandle) {
    if let Some(panel) = app.get_webview_window("panel") {
        let _ = panel.unminimize();
        let _ = panel.show();
        let _ = panel.set_focus();
        hide_pet_surface(app);
        return;
    }

    let was_hidden = app
        .get_webview_window("main")
        .and_then(|win| win.is_visible().ok())
        .map(|visible| !visible)
        .unwrap_or(true);
    if let Some(ball) = app.get_webview_window("ball") {
        let _ = ball.hide();
    }
    restore_main_window(app);
    // Replay the show only when the pet was actually hidden. A Dock click on an
    // already-visible pet must not restart the pop-in animation.
    if was_hidden {
        let _ = app.emit("main-shown", ());
    }
}

/// Recover a window shown outside every display. Only the position moves here:
/// this runs on a 2-second timer, so it must not resize a window the user is
/// dragging or resizing.
pub fn keep_windows_visible(app: &AppHandle) {
    for label in ["main", "ball"] {
        if let Some(win) = app.get_webview_window(label) {
            let _ = clamp_position(&win);
        }
    }
}

pub fn start_visibility_watch(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(2));
        loop {
            ticker.tick().await;
            keep_windows_visible(&app);
        }
    });
}

/// Focus an existing window with `label`, or build a new centered, resizable one
/// loading `index.html?window=<label>`. Shared by the panel and debug windows.
fn open_or_focus(app: &AppHandle, label: &str, title: &str, w: f64, h: f64) -> Result<(), String> {
    if let Some(win) = app.get_webview_window(label) {
        return win.set_focus().map_err(|e| e.to_string());
    }
    let url = WebviewUrl::App(format!("index.html?window={}", label).into());
    WebviewWindowBuilder::new(app, label, url)
        .title(title)
        .inner_size(w, h)
        .center()
        .resizable(true)
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Open the panel and swap it in for the pet: the pet window hides while the
/// panel is open and reappears (focused) when the panel closes. Hide rather
/// than destroy so the pet keeps its position, Live2D canvas and in-memory
/// chat state; the app also keeps running because a hidden window still counts
/// as a live one.
#[tauri::command]
pub async fn open_panel(app: AppHandle) -> Result<(), String> {
    let created = app.get_webview_window("panel").is_none();
    open_or_focus(&app, "panel", "Pet", 900.0, 700.0)?;
    // Panel replaces both the pet and the floating ball. Live2D cannot survive
    // offscreen, so tear the canvas down until the panel's Destroyed handler
    // shows the pet again.
    hide_pet_surface(&app);
    if created {
        if let Some(panel) = app.get_webview_window("panel") {
            let app = app.clone();
            panel.on_window_event(move |event| {
                if let tauri::WindowEvent::Destroyed = event {
                    if let Some(main) = app.get_webview_window("main") {
                        let _ = main.show();
                        let _ = main.set_focus();
                    }
                    // The pet window was hidden the whole time the panel was open,
                    // so anything that can't initialize while hidden (Live2D's
                    // WebGL context) needs to be rebuilt now that it's visible.
                    let _ = app.emit("main-shown", ());
                }
            });
        }
    }
    Ok(())
}

/// Open the debug window. Available in debug builds only.
#[cfg(debug_assertions)]
#[tauri::command]
pub async fn open_debug(app: AppHandle) -> Result<(), String> {
    open_or_focus(&app, "debug", "Pet - Debug", 700.0, 500.0)
}

/// Open the web inspector (DevTools) for the window that invoked this command.
/// Available in debug builds, or release builds compiled with the `devtools` feature.
#[tauri::command]
pub fn open_devtools(window: tauri::WebviewWindow) {
    #[cfg(debug_assertions)]
    window.open_devtools();
    #[cfg(not(debug_assertions))]
    let _ = window;
}
