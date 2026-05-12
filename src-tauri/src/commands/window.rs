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

/// 关掉当前 main 窗口然后用 tauri.conf.json 同样的尺寸 / 装饰 / 透明 / pin 配
/// 置重建。给设置页"重启 pet 窗口"按钮用 —— 改了 Live2D 模型 / motion_mapping
/// / config 的 minSize 等需要重启窗口才生效的字段后省用户手动 quit。
///
/// 不关 panel / debug 窗口；它们独立运转。
#[tauri::command]
pub async fn restart_pet_window(app: AppHandle) -> Result<(), String> {
    // 关旧 main（如果还在），忽略关闭错误 —— 可能已经被用户手动关了。
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.close();
    }
    // 重建 main。配置与 tauri.conf.json 的 main window 块保持一致：
    // 300x450 inner_size、min 220x350、装饰关、透明、alwaysOnTop、不入 dock
    // taskbar、无阴影。resizable 与 tauri.conf 中 resizable: true 同步。
    let url = WebviewUrl::App("index.html".into());
    let mut builder = WebviewWindowBuilder::new(&app, "main", url)
        .title("Pet")
        .inner_size(300.0, 450.0)
        .min_inner_size(220.0, 350.0)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .resizable(true)
        .skip_taskbar(true)
        .shadow(false);
    // macOS 私有 API：让窗口在所有 Space / desktops 上可见（不强制，按需启用）。
    // 这里复用 tauri.conf.json "macOSPrivateApi": true 的配置语义 —— builder
    // 上目前没专门一个等价 method，由 conf 全局控制即可。
    let _ = &mut builder;
    builder
        .build()
        .map_err(|e| format!("重建 pet 窗口失败：{}", e))?;
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
