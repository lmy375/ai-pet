//! Window geometry: fit a rectangle onto the monitor work area it overlaps most.
//!
//! All values in this module are **logical** pixels. Physical values read from
//! a window/monitor are converted with their `scale_factor` before they reach
//! these helpers, so a 2x display behaves identically to a 1x one here.

use tauri::{LogicalPosition, LogicalSize, WebviewWindow};

/// `(left, top, width, height)` of a monitor work area, in logical pixels.
pub(crate) type Area = (f64, f64, f64, f64);

/// Fit a rectangle into `area`. A rectangle larger than the area is pinned to
/// the area's top-left corner (it cannot be clamped any further) rather than
/// resized here — resizing is `fit_size`'s job.
pub(crate) fn clamp_into_area(
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    area: Area,
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
pub(crate) fn choose_area(x: f64, y: f64, width: f64, height: f64, areas: &[Area]) -> Option<Area> {
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

/// Position `x`/`y` inside the most relevant work area without resizing.
pub(crate) fn fit_position(
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    areas: &[Area],
) -> Option<LogicalPosition<f64>> {
    let area = choose_area(x, y, width, height, areas)?;
    Some(clamp_into_area(x, y, width, height, area))
}

/// Work areas of every connected display, in logical pixels.
pub(crate) fn monitor_work_areas(win: &WebviewWindow) -> Result<Vec<Area>, String> {
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

/// Read the window's logical geometry plus the work area it belongs to.
fn measure(win: &WebviewWindow) -> Result<(LogicalPosition<f64>, LogicalSize<f64>, Area), String> {
    let scale = win.scale_factor().map_err(|e| e.to_string())?;
    let position = win
        .outer_position()
        .map_err(|e| e.to_string())?
        .to_logical::<f64>(scale);
    let size = win
        .outer_size()
        .map_err(|e| e.to_string())?
        .to_logical::<f64>(scale);
    let areas = monitor_work_areas(win)?;
    let area = choose_area(position.x, position.y, size.width, size.height, &areas)
        .ok_or_else(|| "no display available".to_string())?;
    Ok((position, size, area))
}

/// Move the window to `target` only when it actually shifts.
fn place(
    win: &WebviewWindow,
    current: LogicalPosition<f64>,
    size: LogicalSize<f64>,
    area: Area,
) -> Result<LogicalPosition<f64>, String> {
    let target = clamp_into_area(current.x, current.y, size.width, size.height, area);
    if (current.x - target.x).abs() > 0.5 || (current.y - target.y).abs() > 0.5 {
        win.set_position(target).map_err(|e| e.to_string())?;
    }
    Ok(target)
}

/// Move the window fully inside a display. Never changes its size, so callers
/// reacting to move/resize events cannot trigger a second resize.
pub(crate) fn clamp_position(win: &WebviewWindow) -> Result<LogicalPosition<f64>, String> {
    let (current, size, area) = measure(win)?;
    place(win, current, size, area)
}

/// Shrink the window into its work area when it is larger, then move it inside.
/// This is the explicit resize path (saving/restoring a position); the periodic
/// visibility poll must use `clamp_position` instead.
pub(crate) fn fit_size(win: &WebviewWindow) -> Result<LogicalPosition<f64>, String> {
    let (current, mut size, area) = measure(win)?;
    if size.width > area.2 || size.height > area.3 {
        size = LogicalSize::new(size.width.min(area.2), size.height.min(area.3));
        win.set_size(size).map_err(|e| e.to_string())?;
    }
    place(win, current, size, area)
}

#[cfg(test)]
mod tests {
    use super::{choose_area, clamp_into_area, fit_position};
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

    /// A window left of a negative-origin display is pulled into that display
    /// instead of crossing over to the primary one.
    #[test]
    fn a_window_offscreen_left_of_a_negative_display_stays_on_it() {
        let areas = areas();
        let area = choose_area(-1000.0, -500.0, 320.0, 560.0, &areas).unwrap();
        assert_eq!(area, (-262.0, -1080.0, 1920.0, 1080.0));

        let pos = fit_position(-1000.0, -500.0, 320.0, 560.0, &areas).unwrap();
        assert_eq!((pos.x, pos.y), (-262.0, -560.0));
        assert!(pos.x >= -262.0 && pos.y >= -1080.0);
    }

    /// The helpers take logical coordinates only; a 2x display is represented
    /// by halved physical dimensions and behaves exactly like a 1x display.
    #[test]
    fn a_2x_display_uses_logical_coordinates() {
        // 2880x1800 @2x primary, 4K left-hand display @2x.
        let areas = vec![
            (0.0, 0.0, 1440.0, 900.0),
            (-1920.0, -1080.0, 1920.0, 1080.0),
        ];
        let pos = fit_position(-2000.0, -100.0, 320.0, 560.0, &areas).unwrap();
        assert_eq!((pos.x, pos.y), (-1920.0, -560.0));
        assert!(pos.x >= -1920.0 && pos.y >= -1080.0);
        assert!(pos.x + 320.0 <= 0.0 && pos.y + 560.0 <= 0.0);
    }

    /// A window wider/taller than its work area cannot be made fully visible by
    /// moving alone: it is pinned to the area's top-left corner.
    #[test]
    fn a_window_larger_than_the_work_area_is_pinned_to_its_origin() {
        let area = areas()[0];
        let pos = clamp_into_area(100.0, 100.0, 2000.0, 1200.0, area);
        assert_eq!((pos.x, pos.y), (0.0, 0.0));

        let fitted = fit_position(100.0, 100.0, 2000.0, 1200.0, &areas()).unwrap();
        assert_eq!((fitted.x, fitted.y), (0.0, 0.0));
    }
}
