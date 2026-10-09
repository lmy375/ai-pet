//! Low-level AX plumbing shared by the snapshot walker and the input layer:
//! app resolution, permission check, and per-element attribute reads.

use accessibility::{AXUIElement, AXUIElementAttributes};
use core_foundation::base::{CFType, TCFType};
use core_foundation::dictionary::CFDictionary;
use core_foundation::number::CFNumber;
use core_foundation::string::{CFString, CFStringRef};
use core_graphics::window::{
    copy_window_info, kCGWindowListExcludeDesktopElements, kCGWindowListOptionOnScreenOnly,
    kCGWindowOwnerName, kCGWindowOwnerPID, kCGNullWindowID,
};
use core_graphics_types::geometry::CGRect;

use crate::AxError;

/// True if this process may use the AX API; on first denial, ask macOS to
/// show its "grant Accessibility" guidance dialog.
pub fn ensure_trusted() -> Result<(), AxError> {
    if unsafe { accessibility_sys::AXIsProcessTrusted() } {
        return Ok(());
    }
    let key =
        unsafe { CFString::wrap_under_get_rule(accessibility_sys::kAXTrustedCheckOptionPrompt) };
    let value = core_foundation::boolean::CFBoolean::true_value().as_CFType();
    let opts = CFDictionary::from_CFType_pairs(&[(key, value)]);
    if unsafe { accessibility_sys::AXIsProcessTrustedWithOptions(opts.as_concrete_TypeRef()) } {
        Ok(())
    } else {
        Err(AxError::NotTrusted)
    }
}

/// Resolve an app name (case-insensitive substring of the window-owner name,
/// which stays stable across locales where possible) to its pid and AX
/// application element, with a short messaging timeout so an unresponsive
/// target can't hang every call.
pub fn find_app(name: &str) -> Result<(i32, AXUIElement), AxError> {
    let options = kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements;
    let windows =
        copy_window_info(options, kCGNullWindowID).ok_or_else(|| AxError::AppNotFound(name.into()))?;
    let target = name.to_lowercase();
    for w in windows.iter() {
        // Untyped CFArray (default item type = CFTypeRef): wrap the raw
        // pointer, then downcast into the window-info dictionary. Windows
        // lacking owner/pid fields are skipped, not fatal.
        let cf = unsafe { CFType::wrap_under_get_rule(*w) };
        let Some(dict) = cf.downcast_into::<UntypedDict>() else { continue };
        let Some(owner) = dict_cf(&dict, unsafe { kCGWindowOwnerName })
            .and_then(|v| v.downcast_into::<CFString>())
        else {
            continue;
        };
        if !owner.to_string().to_lowercase().contains(&target) {
            continue;
        }
        let Some(pid_num) = dict_cf(&dict, unsafe { kCGWindowOwnerPID })
            .and_then(|v| v.downcast_into::<CFNumber>())
        else {
            continue;
        };
        let Some(pid_f) = pid_num.to_f64() else { continue };
        let pid = pid_f as i32;
        let app = AXUIElement::application(pid);
        app.set_messaging_timeout(0.5)
            .map_err(|e| AxError::Ax(e.to_string()))?;
        return Ok((pid, app));
    }
    Err(AxError::AppNotFound(name.into()))
}

/// The app's on-screen window: `focused_window` first (the reliable entry),
/// falling back to the first of `windows`. A result whose role is still
/// AXApplication means the app's AX server has degenerated — usually a hung
/// app; the caller reports it as unresponsive.
pub fn app_window(app: &AXUIElement) -> Result<AXUIElement, AxError> {
    let role_is = |el: &AXUIElement, want: &str| {
        el.role().ok().map(|r| r.to_string()).as_deref() == Some(want)
    };
    let candidates = [
        app.focused_window().ok(),
        app.windows().ok().and_then(|w| w.get(0).map(|e| e.to_owned())),
    ];
    for win in candidates.into_iter().flatten() {
        if role_is(&win, "AXApplication") {
            return Err(AxError::AppUnresponsive);
        }
        if role_is(&win, "AXWindow") {
            return Ok(win);
        }
    }
    Err(AxError::NoWindow)
}

/// The window-info dictionaries come back from an untyped CFArray; work with
/// them in their untyped form (the typed `CFDictionary<K, V>` lacks the
/// `ConcreteCFType` impl `downcast_into` requires).
type UntypedDict = CFDictionary<*const core::ffi::c_void, *const core::ffi::c_void>;

fn dict_cf(dict: &UntypedDict, key: CFStringRef) -> Option<CFType> {
    let v = *dict.find(key as *const core::ffi::c_void)?;
    Some(unsafe { CFType::wrap_under_get_rule(v) })
}

/// The published accessibility 0.2.0 crate doesn't wrap `AXFrame`, so read it
/// through the sys layer: copy the raw attribute value (an AXValue wrapping a
/// CGRect) and unpack it.
pub fn element_frame(element: &AXUIElement) -> Option<CGRect> {
    use accessibility_sys::{
        kAXValueTypeCGRect, AXUIElementCopyAttributeValue, AXValueGetValue,
    };
    use core_foundation_sys::base::CFRelease;

    let name = CFString::new("AXFrame");
    let mut value: core_foundation_sys::base::CFTypeRef = std::ptr::null();
    let err = unsafe {
        AXUIElementCopyAttributeValue(
            element.as_concrete_TypeRef(),
            name.as_concrete_TypeRef(),
            &mut value,
        )
    };
    if err != accessibility_sys::kAXErrorSuccess || value.is_null() {
        return None;
    }
    let mut rect = CGRect::default();
    let ok = unsafe {
        AXValueGetValue(
            value as accessibility_sys::AXValueRef,
            kAXValueTypeCGRect as accessibility_sys::AXValueType,
            &mut rect as *mut _ as *mut core::ffi::c_void,
        )
    };
    unsafe { CFRelease(value) };
    ok.then_some(rect)
}

/// The element's display text: title, description, and (string) value joined
/// with " | ", empty parts dropped.
pub fn element_text(element: &AXUIElement) -> String {
    let parts = [
        element.title().ok().map(|t| t.to_string()),
        element.description().ok().map(|d| d.to_string()),
        element
            .value()
            .ok()
            .and_then(|v| v.downcast_into::<CFString>().map(|s| s.to_string())),
    ];
    parts
        .into_iter()
        .flatten()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" | ")
}

pub fn element_role(element: &AXUIElement) -> String {
    element.role().ok().map(|r| r.to_string()).unwrap_or_default()
}

pub fn element_actions(element: &AXUIElement) -> Vec<String> {
    element
        .action_names()
        .map(|names| names.iter().map(|n| n.to_string()).collect())
        .unwrap_or_default()
}

pub fn frame_string(frame: CGRect) -> String {
    format!(
        "({:.0},{:.0} {:.0}x{:.0})",
        frame.origin.x, frame.origin.y, frame.size.width, frame.size.height
    )
}
