//! Floating ball window: the pet collapses into it when idle.

use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use super::geometry::{fit_position, monitor_work_areas};
use super::position::load_ball_position;

/// Logical size of the ball window; the frontend renders a round ball inside it.
/// Slightly larger than the ball so its drop shadow renders un-clipped — a
/// shadow cut by the window edge reads as a gray ring (the bug this fixed).
const BALL_SIZE: f64 = 64.0;

/// Vertical offset (logical px) from the top of the pet window to the center of
/// the pet's face. `tauri.conf.json` pins the `main` window to 320×560 with the
/// Live2D face near the top; this is where the ball lands when no position has
/// been saved yet. The horizontal center is derived from the window's actual
/// (undecorated, so outer == inner) width, so a resized pet window still
/// collapses onto its face.
const PET_FACE_CENTER_Y: f64 = 96.0;

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
            // Land on the pet's face: the horizontal center comes from the
            // window's actual width, the face sits `PET_FACE_CENTER_Y` below its
            // top, and the ball centers on that point.
            let scale = main.scale_factor().map_err(|e| e.to_string())?;
            let pos = main
                .outer_position()
                .map_err(|e| e.to_string())?
                .to_logical::<f64>(scale);
            let size = main
                .outer_size()
                .map_err(|e| e.to_string())?
                .to_logical::<f64>(scale);
            fit_position(
                pos.x + size.width / 2.0 - BALL_SIZE / 2.0,
                pos.y + PET_FACE_CENTER_Y - BALL_SIZE / 2.0,
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
    super::surface::present_foreground(&app);
    Ok(())
}
