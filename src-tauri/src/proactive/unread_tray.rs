//! 053-part2: unread proactive utterance counter + tray tooltip.
//!
//! 主窗口隐藏时 pet 主动开口 / morning_briefing / welcome_back 会 emit
//! `proactive-message`；用户看不到 ChatMini，需要 tray 给个被动信号。
//! 调 [`record_emitted`] 检查 main window 可见性，若不可见则 bump 计数
//! + 更新 tray tooltip 为「Pet · N 条未读」。main window 重新 focus 后
//! [`clear`] 复原 tooltip 与计数。

use std::sync::atomic::{AtomicU64, Ordering};

use tauri::{AppHandle, Manager};

static UNREAD_COUNT: AtomicU64 = AtomicU64::new(0);

const TRAY_ID: &str = "main-tray";
const DEFAULT_TOOLTIP: &str = "Pet";

/// Proactive emit 后调一次。若 main window 隐藏则 bump 计数 + 更新 tray
/// tooltip；可见则 noop（用户看到 ChatMini 即视为「已读」）。Window 未
/// 创建 / tray 未 build / set_tooltip 失败一律静默吞 — 兜底显示退化无害。
pub fn record_emitted(app: &AppHandle) {
    let visible = app
        .get_webview_window("main")
        .and_then(|w| w.is_visible().ok())
        .unwrap_or(false);
    if visible {
        return;
    }
    let n = UNREAD_COUNT.fetch_add(1, Ordering::Relaxed) + 1;
    update_tooltip(app, n);
}

/// Main window 重新 focus / show 时调。计数清零，tooltip 复原。重复调用
/// 安全（atomic store + tooltip 写一次都是幂等）。
pub fn clear(app: &AppHandle) {
    let prev = UNREAD_COUNT.swap(0, Ordering::Relaxed);
    if prev == 0 {
        return; // 无未读 → tooltip 已经是默认值
    }
    update_tooltip(app, 0);
}

fn update_tooltip(app: &AppHandle, count: u64) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let text = if count == 0 {
        DEFAULT_TOOLTIP.to_string()
    } else {
        format!("{} · {} 条未读", DEFAULT_TOOLTIP, count)
    };
    let _ = tray.set_tooltip(Some(text));
}

