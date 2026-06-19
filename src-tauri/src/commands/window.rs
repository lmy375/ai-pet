use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
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

// --- Pet window position persistence ---

#[derive(Serialize, Deserialize)]
pub struct WindowPosition {
    pub x: i32,
    pub y: i32,
}

fn window_state_path() -> Result<PathBuf, String> {
    Ok(crate::common::config_dir()?.join("window_state.json"))
}

/// Persist the pet window's top-left position so it reopens where the user left
/// it. Called (debounced) from the frontend whenever the user moves the window.
#[tauri::command]
pub fn save_window_position(x: i32, y: i32) -> Result<(), String> {
    let path = window_state_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create config dir: {}", e))?;
    }
    let json = serde_json::to_string(&WindowPosition { x, y })
        .map_err(|e| format!("Failed to serialize window state: {}", e))?;
    fs::write(&path, json).map_err(|e| format!("Failed to write window state: {}", e))
}

fn load_window_position() -> Option<WindowPosition> {
    let content = fs::read_to_string(window_state_path().ok()?).ok()?;
    serde_json::from_str(&content).ok()
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

#[tauri::command]
pub async fn open_panel(app: AppHandle) -> Result<(), String> {
    // If panel already exists, just focus it
    if let Some(win) = app.get_webview_window("panel") {
        win.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }

    let url = WebviewUrl::App("index.html?window=panel".into());

    WebviewWindowBuilder::new(&app, "panel", url)
        .title("Pet - Panel")
        .inner_size(900.0, 700.0)
        .center()
        .resizable(true)
        .build()
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn open_debug(app: AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("debug") {
        win.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }

    let url = WebviewUrl::App("index.html?window=debug".into());

    WebviewWindowBuilder::new(&app, "debug", url)
        .title("Pet - Debug")
        .inner_size(700.0, 500.0)
        .center()
        .resizable(true)
        .build()
        .map_err(|e| e.to_string())?;

    Ok(())
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
