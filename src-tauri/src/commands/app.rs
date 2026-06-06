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

/// GOAL 045：列出当前 LLM provider 可用模型供 PanelSettings 下拉选用。
/// 调 `{api_base}/models`（与 ping_llm 同端点）；解析 `data[]` 数组里的
/// `id` 字段（OpenAI / Anthropic / 多数 compat 都用该 shape）。失败时返
/// 友好 error，由前端兜底"模型列表拉不到，正在用 <当前 model_id>"提示。
///
/// 过滤策略：
/// - id 必须非空（否则跳过）；
/// - id 前缀含 `dall-e` / `whisper` / `tts` / `embedding` 一律滤掉（非 chat 模型）；
/// - 排序：含当前 settings.model 的条目排第一（让 owner 一眼看到自己在用什么），
///   其余按 id 字符串排（稳定可预期）。
#[tauri::command]
pub async fn list_available_models() -> Result<Vec<AvailableModel>, String> {
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
    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", settings.api_key))
        .header("x-api-key", &settings.api_key) // Anthropic 端点用 x-api-key
        .header("anthropic-version", "2023-06-01")
        .send()
        .await
        .map_err(|e| format!("请求 {} 失败：{}", url, e))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!(
            "拉取模型列表失败：HTTP {}（{}）",
            status.as_u16(),
            url
        ));
    }
    let json: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("解析 /models 响应失败：{}", e))?;
    let raw_items = json
        .get("data")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let current_model = settings.model.trim().to_string();
    let models = filter_and_sort_models(&raw_items, &current_model);
    Ok(models)
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AvailableModel {
    pub id: String,
    /// 简短描述。OpenAI shape 没有 description；Anthropic 有 `display_name`。
    /// 抽不到时空串。
    #[serde(default)]
    pub description: String,
    /// 是否为当前 settings.model；前端用来高亮"当前在用"。
    pub is_current: bool,
}

/// Pure：过滤掉非 chat 模型 + 空 id；当前 model 排第一，其余按 id 升序。
/// raw_items 是 /models 响应 `data[]` 解析后的 JSON object 数组。
pub fn filter_and_sort_models(
    raw_items: &[serde_json::Value],
    current_model: &str,
) -> Vec<AvailableModel> {
    let mut out: Vec<AvailableModel> = raw_items
        .iter()
        .filter_map(|m| {
            let id = m.get("id").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
            if id.is_empty() {
                return None;
            }
            if is_non_chat_model(&id) {
                return None;
            }
            let description = m
                .get("display_name")
                .or_else(|| m.get("description"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let is_current = !current_model.is_empty() && id.eq_ignore_ascii_case(current_model);
            Some(AvailableModel {
                id,
                description,
                is_current,
            })
        })
        .collect();
    out.sort_by(|a, b| match (a.is_current, b.is_current) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.id.cmp(&b.id),
    });
    out
}

/// Pure：判定 id 是否非 chat 模型——image / audio / embedding 一律滤。
/// 用前缀 / 子串匹配；新厂商命名风格如有差异可扩展本表。
pub fn is_non_chat_model(id: &str) -> bool {
    let lower = id.to_ascii_lowercase();
    [
        "dall-e",
        "whisper",
        "tts",
        "embedding",
        "embed-",
        "moderation",
        "davinci-002", // 老 instruct，OpenAI 已标 legacy
    ]
    .iter()
    .any(|prefix| lower.contains(prefix))
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
    use serde_json::json;

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

    #[test]
    fn is_non_chat_model_filters_known_non_chat_prefixes() {
        assert!(is_non_chat_model("dall-e-3"));
        assert!(is_non_chat_model("whisper-1"));
        assert!(is_non_chat_model("tts-1"));
        assert!(is_non_chat_model("text-embedding-3-small"));
        assert!(is_non_chat_model("text-moderation-latest"));
        assert!(!is_non_chat_model("claude-opus-4-7"));
        assert!(!is_non_chat_model("gpt-4o"));
        assert!(!is_non_chat_model("claude-haiku-4-5-20251001"));
    }

    #[test]
    fn filter_and_sort_models_skips_empty_id_and_non_chat() {
        let items = vec![
            json!({"id": "claude-opus-4-7"}),
            json!({"id": ""}),
            json!({"id": "dall-e-3"}),
            json!({"id": "gpt-4o"}),
            json!({}), // 缺 id
        ];
        let out = filter_and_sort_models(&items, "");
        let ids: Vec<&str> = out.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids.len(), 2);
        // 按 id 升序
        assert_eq!(ids[0], "claude-opus-4-7");
        assert_eq!(ids[1], "gpt-4o");
    }

    #[test]
    fn filter_and_sort_models_puts_current_model_first() {
        let items = vec![
            json!({"id": "claude-haiku-4-5"}),
            json!({"id": "claude-opus-4-7"}),
            json!({"id": "claude-sonnet-4-6"}),
        ];
        let out = filter_and_sort_models(&items, "claude-sonnet-4-6");
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].id, "claude-sonnet-4-6");
        assert!(out[0].is_current);
        // 其余按 id 升序
        assert_eq!(out[1].id, "claude-haiku-4-5");
        assert!(!out[1].is_current);
        assert_eq!(out[2].id, "claude-opus-4-7");
    }

    #[test]
    fn filter_and_sort_models_current_match_case_insensitive() {
        let items = vec![json!({"id": "GPT-4O"})];
        let out = filter_and_sort_models(&items, "gpt-4o");
        assert_eq!(out.len(), 1);
        assert!(out[0].is_current);
    }

    #[test]
    fn filter_and_sort_models_extracts_display_name_or_description() {
        let items = vec![
            json!({"id": "claude-opus-4-7", "display_name": "Claude Opus 4.7"}),
            json!({"id": "gpt-4o", "description": "OpenAI flagship multimodal"}),
            json!({"id": "no-desc"}),
        ];
        let out = filter_and_sort_models(&items, "");
        let desc_map: std::collections::HashMap<&str, &str> = out
            .iter()
            .map(|m| (m.id.as_str(), m.description.as_str()))
            .collect();
        assert_eq!(desc_map.get("claude-opus-4-7"), Some(&"Claude Opus 4.7"));
        assert_eq!(desc_map.get("gpt-4o"), Some(&"OpenAI flagship multimodal"));
        assert_eq!(desc_map.get("no-desc"), Some(&""));
    }

    #[test]
    fn filter_and_sort_models_empty_input_returns_empty() {
        let out = filter_and_sort_models(&[], "");
        assert!(out.is_empty());
    }
}
