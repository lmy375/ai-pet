//! 桌面截屏 → 多模态 user turn 的后端入口（macOS）。
//!
//! 设计选择：直接 shell 到系统自带的 `screencapture`，而非引入 `xcap` /
//! `screenshots` 等 crate。理由：
//! - 零新依赖：macOS 用户必定有 `screencapture` 二进制。
//! - 权限链路天然：macOS TCC 把「Screen Recording」权限挂在调用进程身上
//!   = 我们的 Tauri 主进程。`screencapture` 由我们 spawn，权限继承自父进
//!   程。未授权时它会非零退出 + stderr 含 `not authorized`，前端拿到
//!   Err 后引导用户去系统设置开权限（不静默吞错，对齐 GOAL 002）。
//! - 输出格式可控：`-t png` 主显示器原生分辨率 PNG，再交给共享的
//!   [`resize_and_encode_jpeg`] 走 001 已经验证过的缩放 / 重编码路径。
//!
//! history 二进制不持久化是前端 useChat 的事（参 `useChat.ts`
//! sendMessage 的 `historyText` 选项）；本文件只负责把 in-flight 的图给
//! 出去 —— 返回值是 `data:image/jpeg;base64,...` data URL，可直接塞进
//! ChatMessage 的 multimodal `image_url.url` 字段。

use std::process::Command;

use base64::Engine;

/// 截主显示器当前画面，返回可直接喂给 multimodal LLM 的 data URL。
/// 失败时 Err 字符串面向终端用户（前端把它原样冒泡），所以文案要可读。
#[tauri::command]
pub fn screenshot_capture() -> Result<String, String> {
    if !cfg!(target_os = "macos") {
        return Err("当前平台暂不支持截屏".to_string());
    }

    // 用 system temp_dir + uuid 起一个唯一文件路径；不引 `tempfile` 新
    // 依赖（uuid 是已有直接依赖）。函数末尾显式删，避免 /tmp 残留 PNG。
    let tmp_path = std::env::temp_dir().join(format!(
        "pet-screenshot-{}.png",
        uuid::Uuid::new_v4()
    ));

    // -x: 静音（不播相机快门声）— 用户主动点 📸 已经有 UI 反馈，再叠
    //     声音是噪音。
    // -t png: 显式 PNG（默认也是 PNG，写出来文档化）。
    // -D 1: 主显示器（macOS 文档下标从 1 起）。需求里就是「主显示器当
    //       前截图」，多屏环境时不该让 LLM 看到副屏（隐私 / 上下文偏移）。
    let output = Command::new("screencapture")
        .arg("-x")
        .arg("-t")
        .arg("png")
        .arg("-D")
        .arg("1")
        .arg(&tmp_path)
        .output()
        .map_err(|e| format!("无法调用 screencapture：{}", e))?;

    if !output.status.success() {
        // 区分「权限未授」与其它错误：TCC 拒绝时 stderr 含
        // `not authorized` / `Not authorized` 这类关键词；遇到时返回引
        // 导文案让前端可以做"打开系统设置"按钮。
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.to_lowercase().contains("not authorized")
            || stderr.to_lowercase().contains("permission")
        {
            return Err(
                "截屏权限未授权。请到 系统设置 → 隐私与安全 → 屏幕录制 \
                 给桌面宠物授权后重启应用。"
                    .to_string(),
            );
        }
        return Err(format!(
            "screencapture 失败（exit={:?}）: {}",
            output.status.code(),
            stderr.trim()
        ));
    }

    let png_bytes = std::fs::read(&tmp_path)
        .map_err(|e| format!("读取截图文件失败：{}", e))?;
    // 读完即删；GOAL 002 要求「不持久化二进制」，临时文件也属于持久化
    // surface（重启后 /tmp 残留对隐私不友好）。删失败时只警告不返回错
    // 误：base64 已在内存里，业务已成功。
    if let Err(e) = std::fs::remove_file(&tmp_path) {
        log::warn!("failed to remove screenshot tmp file {:?}: {}", tmp_path, e);
    }

    // 空文件兜底：某些 TCC 拒绝场景 screencapture 会 exit 0 但写出 0
    // 字节文件。视作权限问题，导向同一条引导文案。
    if png_bytes.is_empty() {
        return Err(
            "截屏权限未授权或主显示器无内容。请到 系统设置 → 隐私与安全 → \
             屏幕录制 给桌面宠物授权后重启应用。"
                .to_string(),
        );
    }

    // 复用 TG 那条已经验证的 resize + JPEG 重编码 path（长边 1568, q85）。
    let jpeg = crate::telegram::photo::resize_and_encode_jpeg(&png_bytes)?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&jpeg);
    Ok(format!("data:image/jpeg;base64,{}", b64))
}
