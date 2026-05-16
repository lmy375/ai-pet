//! App-level meta commands（version / build-time 等）。与 commands/window.rs
//! 同层；不归到 db.rs 因为不依赖 SQLite。

/// 返回 Cargo.toml 里声明的 app 版本号（如 "0.1.0"）。前端 PanelSettings 显在
/// SQLite stats chip 行最前面，让用户一眼自检"我跑的是哪个版本"。
///
/// 用 `env!()` 编译期取值 —— 与 build 输出一致，不可能 drift。
#[tauri::command]
pub fn app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// `ping_llm` 命令返回 LLM endpoint 连通性 + 延迟测算。pet 右键菜单
/// 「📡 ping LLM」入口让 owner 在"宠物不回应"时排查 —— 是网络挂了 / api_base
/// 错了 / api_key 错了 / 模型不可用。
///
/// 策略：调 `{api_base}/models` 端点（OpenAI compat 通用 cheap endpoint，
/// 不消耗 token，只列模型清单）+ 计时。失败 / 超时 / 非 2xx 都附原因返
/// owner。10s timeout 防卡死。
#[tauri::command]
pub async fn ping_llm() -> Result<PingLlmResult, String> {
    let settings = crate::commands::settings::get_settings()?;
    let base = settings.api_base.trim_end_matches('/').to_string();
    if base.is_empty() {
        return Err("api_base 未配置".to_string());
    }
    let url = format!("{}/models", base);
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("HTTP client init failed: {e}"))?;
    let started = std::time::Instant::now();
    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", settings.api_key))
        .send()
        .await
        .map_err(|e| format!("请求 {} 失败：{}", url, e))?;
    let elapsed_ms = started.elapsed().as_millis() as u64;
    let status = resp.status();
    let status_code = status.as_u16();
    let ok = status.is_success();
    // 不读 body（即便 200 / 401 都已经能判断"通了"）—— 避免大列表 IO。
    Ok(PingLlmResult {
        ok,
        elapsed_ms,
        status_code,
        api_base: base,
        model: settings.model,
    })
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PingLlmResult {
    /// HTTP 2xx 返 true；4xx/5xx 返 false（但仍 Ok 给前端，让 owner 看 status）。
    pub ok: bool,
    /// Round-trip wall time。
    pub elapsed_ms: u64,
    /// HTTP status code，让 owner 区分 401（key 错）/ 404（path 错）/ 500（服务器问题）等。
    pub status_code: u16,
    /// 回 echo 当前 api_base，确认 owner 知道在 ping 谁。
    pub api_base: String,
    /// 当前配置的 model 名，方便 owner 看到 chat 实际会用哪个。
    pub model: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_version_returns_cargo_pkg_version() {
        // 至少非空 + 符合 semver 字符集（数字 + 点）；具体值随 release bump，
        // 不写死避免每次升版改测试。
        let v = app_version();
        assert!(!v.is_empty(), "version should not be empty");
        assert!(
            v.chars().all(|c| c.is_ascii_digit() || c == '.' || c.is_ascii_alphabetic() || c == '-'),
            "version unexpected charset: {v}"
        );
    }
}
