use std::sync::Mutex;
use tauri::{AppHandle, Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

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

// --- Pet window position persistence (stored in config.yaml) ---

use crate::commands::settings::{get_settings, set_window_position, WindowPosition};

/// Persist the pet window's top-left position so it reopens where the user left
/// it. Called (debounced) from the frontend whenever the user moves the window.
#[tauri::command]
pub fn save_window_position(x: i32, y: i32) -> Result<(), String> {
    set_window_position(x, y)
}

fn load_window_position() -> Option<WindowPosition> {
    get_settings().ok()?.window
}

/// True if `(x, y)` falls within some connected monitor, so a saved position from
/// a now-disconnected display doesn't strand the window offscreen.
fn position_on_screen(win: &WebviewWindow, x: i32, y: i32) -> bool {
    let monitors = match win.available_monitors() {
        Ok(m) => m,
        Err(_) => return true, // can't verify → trust the saved position
    };
    monitors.iter().any(|m| {
        let p = m.position();
        let s = m.size();
        x >= p.x && x < p.x + s.width as i32 && y >= p.y && y < p.y + s.height as i32
    })
}

/// Restore the saved pet-window position (if any and still on-screen), else
/// center it, then show the window. Called once at startup. The window starts
/// hidden (see tauri.conf.json) so it's positioned before it appears, avoiding a
/// flash at the default center. Always shows so a bad saved position never hides
/// the pet.
pub fn restore_main_window(app: &AppHandle) {
    let win = match app.get_webview_window("main") {
        Some(w) => w,
        None => return,
    };
    let restored = load_window_position()
        .filter(|pos| position_on_screen(&win, pos.x, pos.y))
        .and_then(|pos| win.set_position(PhysicalPosition::new(pos.x, pos.y)).ok());
    if restored.is_none() {
        let _ = win.center();
    }
    let _ = win.show();
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

#[tauri::command]
pub async fn open_panel(app: AppHandle) -> Result<(), String> {
    open_or_focus(&app, "panel", "Pet", 900.0, 700.0)
}

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
