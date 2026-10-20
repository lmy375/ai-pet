use std::sync::Mutex;
use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

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
pub fn keep_window_visible(window: WebviewWindow) -> Result<(), String> {
    clamp_window_to_screen(&window).map(|_| ())
}

#[tauri::command]
pub fn save_window_position(window: WebviewWindow) -> Result<(), String> {
    let pos = clamp_window_to_screen(&window)?;
    set_window_position(pos.x.round() as i32, pos.y.round() as i32)
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
pub fn save_ball_position(window: WebviewWindow) -> Result<(), String> {
    let pos = clamp_window_to_screen(&window)?;
    set_ball_position(pos.x.round() as i32, pos.y.round() as i32)
}

/// Fit a rectangle into the monitor work area with the largest overlap. The
/// saved position is only useful when a meaningful part of the window would be
/// visible; clamping makes it fully visible when it is near an edge.
fn clamp_into_area(
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    area: (f64, f64, f64, f64),
) -> LogicalPosition<f64> {
    let (left, top, area_width, area_height) = area;
    let right = left + area_width;
    let bottom = top + area_height;
    LogicalPosition::new(
        x.clamp(left, (right - width).max(left)),
        y.clamp(top, (bottom - height).max(top)),
    )
}

/// Choose the display with the largest overlap, or the nearest one when the
/// rectangle is completely offscreen.
fn choose_area(
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    areas: &[(f64, f64, f64, f64)],
) -> Option<(f64, f64, f64, f64)> {
    let overlap = areas
        .iter()
        .map(|&(ax, ay, aw, ah)| {
            let overlap_w = (x + width).min(ax + aw) - x.max(ax);
            let overlap_h = (y + height).min(ay + ah) - y.max(ay);
            (overlap_w.max(0.0) * overlap_h.max(0.0), ax, ay, aw, ah)
        })
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .filter(|candidate| candidate.0 > 0.0);
    if let Some((_, ax, ay, aw, ah)) = overlap {
        return Some((ax, ay, aw, ah));
    }

    areas
        .iter()
        .min_by(|&&(ax, ay, aw, ah), &&(bx, by, bw, bh)| {
            let distance = |left: f64, top: f64, area_w: f64, area_h: f64| {
                let dx = (left - (x + width)).max(x - (left + area_w)).max(0.0);
                let dy = (top - (y + height)).max(y - (top + area_h)).max(0.0);
                dx * dx + dy * dy
            };
            distance(ax, ay, aw, ah).total_cmp(&distance(bx, by, bw, bh))
        })
        .copied()
}

fn fit_position(
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    areas: &[(f64, f64, f64, f64)],
) -> Option<LogicalPosition<f64>> {
    let area = choose_area(x, y, width, height, areas)?;
    Some(clamp_into_area(x, y, width, height, area))
}

fn clamp_window_to_screen(win: &WebviewWindow) -> Result<LogicalPosition<f64>, String> {
    let scale = win.scale_factor().map_err(|e| e.to_string())?;
    let current = win
        .outer_position()
        .map_err(|e| e.to_string())?
        .to_logical::<f64>(scale);
    let mut size = win
        .outer_size()
        .map_err(|e| e.to_string())?
        .to_logical::<f64>(scale);
    let areas = monitor_work_areas(win)?;
    let area = choose_area(current.x, current.y, size.width, size.height, &areas)
        .ok_or_else(|| "no display available".to_string())?;
    if size.width > area.2 || size.height > area.3 {
        size = LogicalSize::new(size.width.min(area.2), size.height.min(area.3));
        win.set_size(size).map_err(|e| e.to_string())?;
    }
    let target = clamp_into_area(current.x, current.y, size.width, size.height, area);
    if (current.x - target.x).abs() > 0.5 || (current.y - target.y).abs() > 0.5 {
        win.set_position(target).map_err(|e| e.to_string())?;
    }
    Ok(target)
}

/// Recover a window shown outside every display. The periodic caller covers
/// monitor unplug/rearrangement, which produces no window move event.
pub fn keep_windows_visible(app: &AppHandle) {
    for label in ["main", "ball"] {
        if let Some(win) = app.get_webview_window(label) {
            let _ = clamp_window_to_screen(&win);
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

fn monitor_work_areas(win: &WebviewWindow) -> Result<Vec<(f64, f64, f64, f64)>, String> {
    win.available_monitors()
        .map_err(|e| e.to_string())
        .map(|monitors| {
            monitors
                .iter()
                .map(|monitor| {
                    let scale = monitor.scale_factor();
                    let area = monitor.work_area();
                    let pos = area.position.to_logical::<f64>(scale);
                    let size = area.size.to_logical::<f64>(scale);
                    (pos.x, pos.y, size.width, size.height)
                })
                .collect()
        })
}

/// Restore the saved pet-window position when the full window can be fitted
/// onto a connected display; otherwise center it. Saved positions now use
/// logical coordinates, matching the coordinate space macOS uses for windows.
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
    WebviewWindowBuilder::new(
        app,
        "ball",
        WebviewUrl::App("index.html?window=ball".into()),
    )
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

    let areas = monitor_work_areas(&ball)?;
    let saved = load_ball_position()
        .and_then(|p| fit_position(p.x as f64, p.y as f64, BALL_SIZE, BALL_SIZE, &areas));
    let position = match saved {
        Some(pos) => pos,
        None => {
            // Land on the pet's face: main is 320 logical px wide, the face sits
            // ~96 logical px from the top, the ball centers on that point.
            let scale = main.scale_factor().map_err(|e| e.to_string())?;
            let pos = main
                .outer_position()
                .map_err(|e| e.to_string())?
                .to_logical::<f64>(scale);
            fit_position(
                pos.x + 320.0 / 2.0 - BALL_SIZE / 2.0,
                pos.y + 96.0 - BALL_SIZE / 2.0,
                BALL_SIZE,
                BALL_SIZE,
                &areas,
            )
            .ok_or_else(|| "no display available for the floating ball".to_string())?
        }
    };
    ball.set_position(position).map_err(|e| e.to_string())?;

    let _ = app.emit("main-hidden", ());
    let _ = main.hide();
    let _ = ball.show();
    Ok(())
}

/// Expand out of the ball (the ball's click handler). If the panel is still
/// open it keeps the screen; otherwise the pet returns at its previous position.
/// `present_foreground` emits `main-shown` only in that second case, so Live2D
/// is rebuilt without the panel path stealing focus back onto a hidden pet.
#[tauri::command]
pub async fn expand_from_ball(app: AppHandle) -> Result<(), String> {
    if app.get_webview_window("main").is_none() {
        return Err("main window missing".to_string());
    }
    present_foreground(&app);
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

#[cfg(test)]
mod tests {
    use super::fit_position;
    use tauri::LogicalPosition;

    fn areas() -> Vec<(f64, f64, f64, f64)> {
        vec![(0.0, 0.0, 1512.0, 982.0), (-262.0, -1080.0, 1920.0, 1080.0)]
    }

    fn assert_fully_visible(pos: LogicalPosition<f64>, width: f64, height: f64) {
        assert!(areas().iter().any(|&(ax, ay, aw, ah)| {
            pos.x >= ax && pos.y >= ay && pos.x + width <= ax + aw && pos.y + height <= ay + ah
        }));
    }

    #[test]
    fn an_offscreen_position_returns_to_the_nearest_display() {
        let main = fit_position(1690.0, 368.0, 320.0, 560.0, &areas()).unwrap();
        assert_fully_visible(main, 320.0, 560.0);
        assert_eq!((main.x, main.y), (1192.0, 368.0));

        let ball = fit_position(2576.0, 902.0, 64.0, 64.0, &areas()).unwrap();
        assert_fully_visible(ball, 64.0, 64.0);
    }

    #[test]
    fn a_partly_visible_position_is_clamped_into_the_monitor() {
        let pos = fit_position(1500.0, 900.0, 320.0, 560.0, &areas()).unwrap();
        assert_eq!((pos.x, pos.y), (1192.0, 422.0));
    }
}
