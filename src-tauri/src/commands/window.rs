use tauri::{AppHandle, Manager, WebviewWindowBuilder, WebviewUrl};

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
