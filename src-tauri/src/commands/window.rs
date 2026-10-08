use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

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

use pet_core::settings::{get_settings, set_ball_position, set_window_position, WindowPosition};

/// Persist the pet window's top-left position so it reopens where the user left
/// it. Called (debounced) from the frontend whenever the user moves the window.
#[tauri::command]
pub fn save_window_position(x: i32, y: i32) -> Result<(), String> {
    set_window_position(x, y)
}

fn load_window_position() -> Option<WindowPosition> {
    get_settings().ok()?.window
}

/// Saved floating-ball position (see `save_ball_position`).
fn load_ball_position() -> Option<WindowPosition> {
    get_settings().ok()?.ball
}

/// Persist the ball's top-left position so it reappears where the user dragged
/// it. Called (debounced) from the ball webview after a drag.
#[tauri::command]
pub fn save_ball_position(x: i32, y: i32) -> Result<(), String> {
    set_ball_position(x, y)
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

// --- Floating ball (the pet collapses into it when idle) ---

/// Logical size of the ball window; the frontend renders a round ball inside it.
/// Slightly larger than the ball so its drop shadow renders un-clipped — a
/// shadow cut by the window edge reads as a gray ring (the bug this fixed).
const BALL_SIZE: f64 = 64.0;

/// Return the existing ball webview, creating it on first use. Created hidden so
/// `collapse_to_ball` can position it before it pops in. It stays alive after
/// the pet expands back (just hidden), so repeated collapses are cheap.
fn ensure_ball_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    if let Some(win) = app.get_webview_window("ball") {
        return Ok(win);
    }
    WebviewWindowBuilder::new(app, "ball", WebviewUrl::App("index.html?window=ball".into()))
        .title("Pet")
        .inner_size(BALL_SIZE, BALL_SIZE)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .shadow(false)
        .visible(false)
        .focused(false)
        .build()
        .map_err(|e| e.to_string())
}

/// Collapse the pet into the floating ball: show the ball (at the user's saved
/// spot, else where the pet is so it reads as the pet shrinking into the ball),
/// then hide the pet window. Emits `main-hidden` first so the frontend tears the
/// Live2D canvas down — a WebGL context can't survive the window being hidden
/// (same mechanism the panel uses; see `open_panel`).
#[tauri::command]
pub async fn collapse_to_ball(app: AppHandle) -> Result<(), String> {
    let main = match app.get_webview_window("main") {
        Some(w) => w,
        None => return Err("main window missing".to_string()),
    };
    let ball = ensure_ball_window(&app)?;

    let saved = load_ball_position().filter(|p| position_on_screen(&ball, p.x, p.y));
    let (x, y) = match saved {
        Some(p) => (p.x, p.y),
        None => {
            // Land on the pet's face: main is 320 logical px wide, the face sits
            // ~96 logical px from the top, the ball centers on that point.
            let scale = main.scale_factor().map_err(|e| e.to_string())?;
            let pos = main.outer_position().map_err(|e| e.to_string())?;
            let size = BALL_SIZE * scale;
            let cx = pos.x as f64 + (320.0 / 2.0) * scale;
            let cy = pos.y as f64 + 96.0 * scale;
            ((cx - size / 2.0).round() as i32, (cy - size / 2.0).round() as i32)
        }
    };
    ball.set_position(PhysicalPosition::new(x, y))
        .map_err(|e| e.to_string())?;

    let _ = app.emit("main-hidden", ());
    let _ = main.hide();
    let _ = ball.show();
    Ok(())
}

/// Expand the pet back out of the ball (the ball's click handler): hide the
/// ball, show the pet at its previous position and refocus it so the idle timer
/// doesn't instantly collapse it again. Emits `main-shown` so the frontend
/// rebuilds the Live2D canvas and replays the pop-in.
#[tauri::command]
pub async fn expand_from_ball(app: AppHandle) -> Result<(), String> {
    let main = app
        .get_webview_window("main")
        .ok_or_else(|| "main window missing".to_string())?;
    if let Some(ball) = app.get_webview_window("ball") {
        let _ = ball.hide();
    }
    let _ = main.show();
    let _ = main.set_focus();
    let _ = app.emit("main-shown", ());
    Ok(())
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
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.hide();
    }
    // The pet window is now hidden. Its Live2D WebGL context cannot survive
    // offscreen, so tell the frontend to tear the canvas down until it's shown
    // again (see the panel's Destroyed handler) rather than rebuilding it here.
    let _ = app.emit("main-hidden", ());
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
