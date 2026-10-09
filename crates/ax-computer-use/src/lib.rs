//! ax-computer-use: macOS computer-use primitives over the Accessibility API.
//!
//! The entry point is [`get_app_state`], which resolves an app by name and
//! renders its front window as a text tree with stable element indices. The
//! action methods on [`AppState`] then reference elements by that index:
//!
//! ```no_run
//! # fn main() -> Result<(), ax_computer_use::AxError> {
//! let state = ax_computer_use::get_app_state("Safari")?;
//! println!("{}", state.render());
//! state.click(3)?;
//! # Ok(())
//! # }
//! ```
//!
//! Actions are non-invasive by design: semantic AX actions (`AXPress`,
//! `AXValue` writes, `AXScroll*ByPage`) run in the background without
//! touching the user's cursor or focus; keyboard/mouse events are posted to
//! the target pid only, never to the global HID tap.
//!
//! Measured on macOS 26 (arm64): pid-scoped keyboard events are ignored by
//! ordinary AppKit apps even when frontmost (Calculator and TextEdit both
//! drop them), so [`AppState::type_text`] always prefers `AXValue` writes —
//! the only typing path that works reliably — and [`AppState::press_key`]
//! should be treated as best-effort, for cases the AX API cannot express
//! (canvas apps, global shortcuts).
//!
//! `AppState` holds live AX references and is neither `Send` nor `Sync`; use
//! it on one thread (e.g. inside a `spawn_blocking` closure).

mod ax;
mod input;
mod snapshot;

use accessibility::{AXAttribute, AXUIElement, AXUIElementAttributes};
use core_foundation::base::TCFType;
use core_foundation::string::CFString;

pub use crate::ax::ensure_trusted;

/// Direction for [`AppState::scroll`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollDirection {
    Up,
    Down,
}

#[derive(Debug)]
pub enum AxError {
    /// The Accessibility permission is missing (System Settings › Privacy &
    /// Security › Accessibility).
    NotTrusted,
    /// No on-screen window owner matches the app name.
    AppNotFound(String),
    /// The app's AX server has degenerated (windows resolve to the
    /// application element itself) — usually a hung app; restart it.
    AppUnresponsive,
    /// The app has no AXWindow to work with.
    NoWindow,
    /// The element index isn't in the current snapshot.
    UnknownIndex(usize),
    /// The element exposes no way to click it (no AX action, no frame).
    NoAction(usize),
    /// A key/combo string failed to parse.
    BadKey(String),
    /// An underlying AX API error.
    Ax(String),
}

impl std::fmt::Display for AxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AxError::NotTrusted => write!(
                f,
                "Accessibility permission missing — grant it in System Settings > Privacy & Security > Accessibility, then retry"
            ),
            AxError::AppNotFound(name) => {
                write!(f, "no on-screen app window matches {name:?}")
            }
            AxError::AppUnresponsive => write!(
                f,
                "the app's accessibility server is unresponsive — restart the app and retry"
            ),
            AxError::NoWindow => write!(f, "the app has no accessible window"),
            AxError::UnknownIndex(i) => write!(
                f,
                "element index {i} is not in the current snapshot — run state again and use a fresh index"
            ),
            AxError::NoAction(i) => write!(
                f,
                "element {i} exposes no click action and has no frame to click at"
            ),
            AxError::BadKey(k) => write!(f, "unrecognized key or combo {k:?}"),
            AxError::Ax(e) => write!(f, "accessibility error: {e}"),
        }
    }
}

impl std::error::Error for AxError {}

/// A resolved app: its pid, window, and the walked element tree. The index
/// printed by [`AppState::render`] is the position in the walked tree and
/// stays valid until the app's UI changes.
pub struct AppState {
    pid: i32,
    app_name: String,
    window_title: String,
    snap: snapshot::Snapshot,
}

/// Resolve an app by name (case-insensitive substring of the on-screen
/// window owner, e.g. "safari" matches "Safari") and walk its front window.
pub fn get_app_state(app: &str) -> Result<AppState, AxError> {
    ensure_trusted()?;
    let (pid, app_element) = ax::find_app(app)?;
    let window = ax::app_window(&app_element)?;
    let window_title = window
        .title()
        .ok()
        .map(|t| t.to_string())
        .unwrap_or_else(|| app.to_string());
    let snap = snapshot::snapshot(&window);
    Ok(AppState { pid, app_name: app.to_string(), window_title, snap })
}

/// An on-screen app as reported by [`list_apps`]: its pid and window-owner
/// name — the same name [`get_app_state`] matches against.
pub struct AppInfo {
    pid: i32,
    name: String,
}

impl AppInfo {
    pub fn pid(&self) -> i32 {
        self.pid
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

/// The on-screen apps, frontmost first, deduplicated by pid. Any name listed
/// here is a valid argument to [`get_app_state`].
pub fn list_apps() -> Vec<AppInfo> {
    ax::list_apps()
        .into_iter()
        .map(|(pid, name)| AppInfo { pid, name })
        .collect()
}

impl AppState {
    pub fn pid(&self) -> i32 {
        self.pid
    }

    pub fn app_name(&self) -> &str {
        &self.app_name
    }

    pub fn window_title(&self) -> &str {
        &self.window_title
    }

    /// The rendered tree, one line per element:
    /// `[index] role "text" (x,y w×h) actions=[...]`.
    pub fn render(&self) -> String {
        self.snap.lines.join("\n")
    }

    fn element(&self, index: usize) -> Result<&AXUIElement, AxError> {
        self.snap.elements.get(index).ok_or(AxError::UnknownIndex(index))
    }

    /// Click an element: semantic AX action first (`AXPress`, `AXConfirm`,
    /// `AXOpen` — runs in the background), then a pid-scoped mouse event at
    /// the element's frame center. Returns a description of the path used.
    pub fn click(&self, index: usize) -> Result<String, AxError> {
        let element = self.element(index)?;
        let actions = ax::element_actions(element);
        for action in ["AXPress", "AXConfirm", "AXOpen"] {
            if actions.iter().any(|a| a == action) {
                element
                    .perform_action(&CFString::new(action))
                    .map_err(|e| AxError::Ax(e.to_string()))?;
                return Ok(format!("performed {action} on element {index}"));
            }
        }
        if let Some(frame) = ax::element_frame(element) {
            let center = core_graphics_types::geometry::CGPoint::new(
                frame.origin.x + frame.size.width / 2.0,
                frame.origin.y + frame.size.height / 2.0,
            );
            input::post_click(self.pid, center);
            return Ok(format!(
                "posted mouse click at ({:.0},{:.0}) to pid {} (element {index} had no AX action)",
                center.x, center.y, self.pid
            ));
        }
        Err(AxError::NoAction(index))
    }

    /// Type text into an element. Prefers a direct `AXValue` write (works in
    /// the background, and the only path that works in apps that reject
    /// pid-scoped keyboard events); falls back to AXPress-for-focus plus
    /// keyboard events. Returns a description of the path used.
    pub fn type_text(&self, index: usize, text: &str) -> Result<String, AxError> {
        let element = self.element(index)?;
        if element
            .is_settable(&AXAttribute::value())
            .unwrap_or(false)
        {
            let current = element
                .value()
                .ok()
                .and_then(|v| v.downcast_into::<CFString>().map(|s| s.to_string()))
                .unwrap_or_default();
            let combined = CFString::new(&format!("{current}{text}"));
            element
                .set_attribute(&AXAttribute::value(), combined.as_CFType())
                .map_err(|e| AxError::Ax(e.to_string()))?;
            return Ok(format!("set AXValue of element {index} (appended)"));
        }
        // Focus the element as best we can, then type through CGEvent.
        // Best-effort only: pid-scoped keyboard events are ignored by many
        // apps (see the crate docs) — AXValue above is the reliable path.
        element.perform_action(&CFString::new("AXPress")).ok();
        input::post_text(self.pid, text);
        Ok(format!(
            "posted {text:?} as keyboard events to pid {} — many apps ignore pid-scoped keyboard input, prefer elements whose value is settable",
            self.pid
        ))
    }

    /// Press a key or combo ("enter", "cmd+c", "ctrl+shift+tab") in the app.
    /// Posts to the app's pid only.
    pub fn press_key(&self, key: &str) -> Result<String, AxError> {
        input::post_key(self.pid, key)?;
        Ok(format!("posted {key:?} to pid {}", self.pid))
    }

    /// Scroll an element: `AXScroll*ByPage` action if the element exposes
    /// one, else (or if the action is refused — some apps list it but reject
    /// it) a pid-scoped scroll-wheel event (≈3 lines).
    pub fn scroll(&self, index: usize, direction: ScrollDirection) -> Result<String, AxError> {
        let element = self.element(index)?;
        let action = match direction {
            ScrollDirection::Up => "AXScrollUpByPage",
            ScrollDirection::Down => "AXScrollDownByPage",
        };
        let ax_ok = ax::element_actions(element).iter().any(|a| a == action)
            && element.perform_action(&CFString::new(action)).is_ok();
        if ax_ok {
            return Ok(format!("performed {action} on element {index}"));
        }
        input::post_scroll(self.pid, direction == ScrollDirection::Up);
        Ok(format!(
            "posted scroll-wheel event ({:?}) to pid {}",
            direction, self.pid
        ))
    }
}
