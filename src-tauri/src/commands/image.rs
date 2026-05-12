//! `/image <prompt>` 后端：调 OpenAI compatible images API 生成图片。
//!
//! 与 chat 同源 base_url + api_key（reuse settings），但 model 字段独立
//! `image_model`（默认 `dall-e-3`）。返回 base64 → 前端拼成 data URL 直接塞
//! 进 `ChatItem.images`，不落盘。失败原因尽量原样回前端，让用户能看到 API 错误
//! （key 错、quota 超、prompt policy 拒）而不是吞掉。
//!
//! ## 部分成功语义（n > 1）
//!
//! provider 对批量 n 的支持参差：dall-e-3 仅 n=1，dall-e-2 / SD 通常 1-10，flux
//! 多为 1-4。直接传 `n: 4` 给 dall-e-3 会整批 400，用户失去"画了几张"的反馈。
//! 解法：n > 1 时拆 N 次串行 n=1 调用，按条聚合 urls + errors。每条独立成败，
//! 最终回 `{ urls, errors }`，前端可以同时显"画了 X/N 张"和那 (N-X) 条的失败
//! 原因。n=1 时仍走单次调，errors 永远空。

use serde::{Deserialize, Serialize};

use crate::commands::settings::get_settings;

#[derive(Serialize)]
struct ImageRequest<'a> {
    model: &'a str,
    prompt: &'a str,
    n: u32,
    size: &'a str,
    response_format: &'a str,
}

#[derive(Deserialize)]
struct ImageResponse {
    data: Vec<ImageDatum>,
}

#[derive(Deserialize)]
struct ImageDatum {
    /// base64 编码的图片字节（response_format=b64_json 时）
    #[serde(default)]
    b64_json: Option<String>,
    /// fallback：某些代理返回 url 而非 b64_json，那就直接转交前端 <img>。
    #[serde(default)]
    url: Option<String>,
}

/// 部分成功结果：urls 是成功生成的 data/http URL，errors 是同次调用里失败的
/// 每条原因（带索引前缀 `#i: ...`）。前端用 urls.len() / (urls + errors) 算"画
/// 了 X/N 张"。
#[derive(Serialize)]
pub struct ImageGenerateResult {
    pub urls: Vec<String>,
    pub errors: Vec<String>,
}

/// 后端"硬"上限。前端有 IMAGE_MAX_N=8；后端再 clamp 一次防 IPC 直接被恶意
/// 调用绕过前端守门。各 provider 的实际能力（dall-e-3=1、dall-e-2≤10、SD/
/// flux 多为 1-4）由 API 自己 enforce，这里不替它判断。
pub const IMAGE_HARD_MAX_N: u32 = 10;

/// 单次 n=1 调用：底层 HTTP 路径。run_image_generate 在 n>1 时多次串行调
/// 这个函数聚合结果；n=1 时直接当 Ok(单元素 vec) / Err 走主路径。
async fn fetch_single_image(
    client: &reqwest::Client,
    url: &str,
    api_key: &str,
    model: &str,
    prompt: &str,
    size: &str,
) -> Result<String, String> {
    let body = ImageRequest {
        model,
        prompt,
        n: 1,
        size,
        response_format: "b64_json",
    };
    let resp = client
        .post(url)
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("请求 images API 失败：{e}"))?;
    let status = resp.status();
    let raw = resp
        .text()
        .await
        .map_err(|e| format!("读取响应体失败：{e}"))?;
    if !status.is_success() {
        return Err(format!("images API 返回 {}：{}", status, raw));
    }
    let parsed: ImageResponse = serde_json::from_str(&raw).map_err(|e| {
        format!(
            "解析响应失败：{e}；原始 body 前 200 字：{}",
            &raw.chars().take(200).collect::<String>()
        )
    })?;
    for d in parsed.data {
        if let Some(b64) = d.b64_json {
            return Ok(format!("data:image/png;base64,{}", b64));
        } else if let Some(u) = d.url {
            return Ok(u);
        }
    }
    Err("images API 没有返回图片数据。".to_string())
}

/// Pure 内部 helper：读 settings、clamp n、调 images API、解析返回。Tauri 命令
/// 与 LLM 工具 give_image 共用此函数 —— 双路径同 settings 来源 + 同错误透传，
/// 用户切 model / size / api_key 不需关心是哪条路径触发的生图。
///
/// 外层 `Err` 只在 setup 阶段失败（无 api_key / 无 model / 空 prompt）；网络
/// 或 API 拒绝走 Ok(部分结果)，让 caller 决定是否当作 partial 成功。
pub async fn run_image_generate(
    prompt: &str,
    n: u32,
    size_override: Option<&str>,
) -> Result<ImageGenerateResult, String> {
    let settings = get_settings()?;
    if settings.api_key.is_empty() {
        return Err("API Key 未配置。打开「设置」填好后再试。".to_string());
    }
    let model = settings.image_model.trim();
    if model.is_empty() {
        return Err("image_model 未配置。在 config.yaml 里填一个图像模型名(如 dall-e-3)。".to_string());
    }
    let prompt_trimmed = prompt.trim();
    if prompt_trimmed.is_empty() {
        return Err("prompt 不能为空。".to_string());
    }
    let n_clamped = n.clamp(1, IMAGE_HARD_MAX_N);
    // size_override 优先 settings.image_size 优先 1024x1024 fallback。前端传过
    // 来的 -s WxH 已经做过粗略格式校验；后端不再 enforce 让 provider 自己拒
    // 不支持的尺寸 —— 错误透传到 errors 列表。
    let size = size_override
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| {
            if settings.image_size.trim().is_empty() {
                "1024x1024"
            } else {
                settings.image_size.trim()
            }
        });
    let url = format!(
        "{}/images/generations",
        settings.api_base.trim_end_matches('/')
    );

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| format!("HTTP client init failed: {e}"))?;

    let mut urls = Vec::with_capacity(n_clamped as usize);
    let mut errors = Vec::new();
    // 串行循环，每次 n=1。dall-e-2 之类支持批 n 的 provider 这里成本相同
    // （每图付费 vs 一次请求多图 —— 钱一样，多 RTT；但部分成功语义远好于
    // batch all-or-nothing）。
    for i in 0..n_clamped {
        match fetch_single_image(&client, &url, &settings.api_key, model, prompt_trimmed, size).await {
            Ok(u) => urls.push(u),
            Err(e) => errors.push(format!("#{}: {}", i + 1, e)),
        }
    }
    if urls.is_empty() && errors.is_empty() {
        // 极端：n=0 clamp 也应该至少跑一次。理论达不到（clamp 下限 1）；防御一下。
        return Err("images API 没有返回任何结果。".to_string());
    }
    Ok(ImageGenerateResult { urls, errors })
}

/// 触发一次图片生成。`n` 缺省 1，clamp 到 [1, IMAGE_HARD_MAX_N]。返回
/// `{ urls, errors }` 部分成功结构 —— 前端塞 `ChatItem.images` 走 urls，把
/// errors 显在 prompt 旁。setup 失败（无 key / model）仍走外层 `Err(String)`，
/// 前端渲染为重试按钮 + 错误说明的失败行。
#[tauri::command]
pub async fn image_generate(
    prompt: String,
    n: Option<u32>,
    size: Option<String>,
) -> Result<ImageGenerateResult, String> {
    run_image_generate(&prompt, n.unwrap_or(1), size.as_deref()).await
}
