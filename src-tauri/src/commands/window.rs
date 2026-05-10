use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

/// 打开当前 webview 的 devtools 调试器。前端旧实现走 `plugin:webview|
/// internal_toggle_devtools` / `(win as any).openDevtools()` 两条 fallback，
/// 在 Tauri 2 webview 上经常拿不到 —— 由 Rust 这边直接调 `open_devtools()`
/// API 稳定可靠。失败仍返回 Err 让前端 banner 显具体原因。
///
/// 仅 debug 构建编译进 binary —— release 默认不暴露 devtools 表面。
#[tauri::command]
pub async fn open_devtools(_app: AppHandle, window: tauri::Window) -> Result<(), String> {
    #[cfg(any(debug_assertions, feature = "devtools"))]
    {
        // window 的 webview 关联：在 Tauri 2 里通过 webview() 拿到 Webview。
        if let Some(webview) = _app.get_webview_window(window.label()) {
            webview.open_devtools();
            return Ok(());
        }
        Err(format!(
            "Cannot find webview for window label `{}`",
            window.label()
        ))
    }
    #[cfg(not(any(debug_assertions, feature = "devtools")))]
    {
        let _ = window;
        Err("DevTools 仅在 debug 构建中可用。本次 release 构建未启用 webview devtools 特性。".to_string())
    }
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
