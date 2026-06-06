//! GOAL 005「通用任务」第一刀：用户消息含 URL 时在 LLM 调用前抓取
//! 标题 + 正文摘要，prepend 成 system note 让宠物能直接对内容下评论，
//! 而不是凭标题猜。
//!
//! 边界 / 不做：
//! - 不做 robots.txt（个人桌面宠物自己抓自己看，etiquette 由 User-Agent
//!   表明身份）；
//! - 不引 scraper crate —— GOAL 明说「剥广告 / 登录墙不强求」，用粗糙的
//!   regex 拿到「标题 + 主要文字流」就够，依赖更轻、编译时间不涨。
//! - 不持久化抓取内容 —— 注入层只对本轮 LLM 生效。chat.rs / bot.rs 的
//!   持久化路径都在 inject 之前 snapshot session，结构上天然隔离。

use std::time::Duration;

use regex::Regex;

use crate::commands::chat::ChatMessage;

/// 单次最多抓几个 URL。3 是经验值：用户一条消息丢 5+ 条链接的场景很少；
/// 给上限免得意外让一条 paste 引发 N 次外网调用。
pub const MAX_URLS: usize = 3;
/// 单条响应字节上限。1MB 足够覆盖正常 article / blog；超过基本是大文档 /
/// PDF / 二进制；截断节点已记号给 LLM 知道是被砍的。
pub const MAX_BYTES: usize = 1_000_000;
/// 单条抓取超时。10s 给慢服务器一点机会但不至于让对话卡住整轮。
pub const FETCH_TIMEOUT_SECS: u64 = 10;
/// 注入到 system note 的正文上限（字符）。再长 token 成本飙升又稀释指令。
pub const BODY_EXCERPT_CHARS: usize = 2_000;
/// debug / 退出阀：用户在消息里加 `--no-fetch` 后缀时本轮跳过抓取。
pub const NO_FETCH_FLAG: &str = "--no-fetch";

fn url_regex() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| Regex::new(r"https?://[^\s<>'\x22`]+").unwrap())
}

/// 从一段文本按出现顺序提取 URL，按字符串值去重。trailing 标点
/// (`. , ) ] ; !`) 不算 URL 末字符 —— "看这条 https://x.com." 里的句号
/// 应当 strip 掉，否则后续 reqwest 会带个无效尾巴。最多返回 [`MAX_URLS`]
/// 条。
pub fn extract_urls(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for m in url_regex().find_iter(text) {
        let trimmed = m
            .as_str()
            .trim_end_matches(|c: char| matches!(c, '.' | ',' | ')' | ']' | ';' | '!' | '?'))
            .to_string();
        if trimmed.is_empty() {
            continue;
        }
        if out.iter().any(|s| s == &trimmed) {
            continue;
        }
        out.push(trimmed);
        if out.len() >= MAX_URLS {
            break;
        }
    }
    out
}

pub struct UrlContent {
    pub title: String,
    pub body: String,
    /// true = 响应超过 [`MAX_BYTES`] 被截断。LLM 看到「正文不完整」就能调
    /// 整回答口径（避免基于 partial 抓取写"全文如此"的硬结论）。
    pub truncated: bool,
}

pub struct UrlFetchResult {
    pub url: String,
    pub outcome: Result<UrlContent, String>,
}

async fn fetch_one(client: &reqwest::Client, url: &str) -> Result<UrlContent, String> {
    let resp = client
        .get(url)
        .header(
            reqwest::header::USER_AGENT,
            "Mozilla/5.0 (compatible; DesktopPet/0.1; +https://example.com/bot)",
        )
        .send()
        .await
        .map_err(|e| format!("请求失败：{}", e))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("HTTP {}", status));
    }
    let ct = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_lowercase();
    // 没有 Content-Type 时按 textual 试 —— 不少老 server 不发 CT；与其
    // 拒抓不如让 strip_tags 自己去对付乱码。明确二进制（image/* video/*
    // application/octet-stream 等）直接拒，避免把 PDF / 图片字节塞给 LLM。
    let is_textual = ct.is_empty()
        || ct.starts_with("text/")
        || ct.starts_with("application/xhtml+xml")
        || ct.starts_with("application/json")
        || ct.starts_with("application/xml");
    if !is_textual {
        return Err(format!("非文本响应：{}", ct));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("读响应体失败：{}", e))?;
    let truncated = bytes.len() > MAX_BYTES;
    let slice = if truncated { &bytes[..MAX_BYTES] } else { &bytes[..] };
    let raw = String::from_utf8_lossy(slice);
    let (title, body) = extract_title_and_body(&raw);
    Ok(UrlContent {
        title,
        body: truncate_chars(&body, BODY_EXCERPT_CHARS),
        truncated,
    })
}

/// 并发抓 N 条 URL，结果按入参顺序对齐返回。任何一条失败都不污染其它
/// 条：错误以 `outcome: Err(...)` 个别表达，caller 据此构造降级文案。
pub async fn fetch_url_summaries(urls: &[String]) -> Vec<UrlFetchResult> {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(FETCH_TIMEOUT_SECS))
        // redirect 默认 10 跳 —— 够覆盖 t.co / lnkd.in 这类短链层级，
        // 又不让恶意重定向无限钻。
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return urls
                .iter()
                .map(|u| UrlFetchResult {
                    url: u.clone(),
                    outcome: Err(format!("HTTP client 初始化失败：{}", e)),
                })
                .collect();
        }
    };
    let futs = urls.iter().map(|u| {
        let client = &client;
        async move {
            UrlFetchResult {
                url: u.clone(),
                outcome: fetch_one(client, u).await,
            }
        }
    });
    futures_util::future::join_all(futs).await
}

fn extract_title_and_body(html: &str) -> (String, String) {
    static TITLE_RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    static SCRIPT_RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    static TAG_RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let title_re = TITLE_RE.get_or_init(|| Regex::new(r"(?is)<title[^>]*>(.*?)</title>").unwrap());
    // <title> 也得整块挖掉，不然「<title>X</title>」开标签和闭标签之间的
    // X 在后面 tag_re strip 后仍会留在 body 里 —— 与 title 重复，破坏「title
    // 与 body 分别归属」语义（测试 decodes_common_entities pin 这一条）。
    let script_re = SCRIPT_RE.get_or_init(|| {
        Regex::new(r"(?is)<(script|style|noscript|title)[^>]*>.*?</(script|style|noscript|title)>")
            .unwrap()
    });
    let tag_re = TAG_RE.get_or_init(|| Regex::new(r"(?s)<[^>]+>").unwrap());

    let title = title_re
        .captures(html)
        .and_then(|c| c.get(1))
        .map(|m| collapse_ws(&decode_entities(m.as_str())))
        .unwrap_or_default();

    let no_script = script_re.replace_all(html, " ");
    let no_tags = tag_re.replace_all(&no_script, " ");
    let body = collapse_ws(&decode_entities(&no_tags));
    (title, body)
}

fn decode_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

fn collapse_ws(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_space = true;
    for c in s.chars() {
        if c.is_whitespace() {
            if !last_space {
                out.push(' ');
                last_space = true;
            }
        } else {
            out.push(c);
            last_space = false;
        }
    }
    out.trim().to_string()
}

fn truncate_chars(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max {
        s.to_string()
    } else {
        let mut t: String = chars.into_iter().take(max).collect();
        t.push('…');
        t
    }
}

/// 从 ChatMessage.content 提取纯文本（OpenAI compatible multimodal 时只取
/// `type=text` parts；image_url parts 当然没文本可读）。
fn extract_text_from_content(content: &serde_json::Value) -> String {
    if let Some(s) = content.as_str() {
        return s.to_string();
    }
    if let Some(arr) = content.as_array() {
        let mut out = String::new();
        for part in arr {
            if part.get("type").and_then(|v| v.as_str()) == Some("text") {
                if let Some(t) = part.get("text").and_then(|v| v.as_str()) {
                    if !out.is_empty() {
                        out.push('\n');
                    }
                    out.push_str(t);
                }
            }
        }
        return out;
    }
    String::new()
}

/// 把 fetch 结果格式化成给 LLM 看的 system note。失败条目用「抓取失败：
/// <reason>」标明，并在末尾给 LLM 一个软约定：在回复尾部加一行 ⚠️
/// 提示。是软约定不是硬截断 —— LLM 偶尔不照办可接受，对话流不要因抓取
/// 报错而被噪音淹没。
fn format_url_context(results: &[UrlFetchResult]) -> String {
    if results.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    out.push_str("【URL 抓取上下文】用户消息含以下链接，系统已尝试抓取标题与正文摘要供你参考。请基于真实抓取内容回答，而不是凭链接标题猜测。\n");
    let mut any_fail = false;
    let mut failed_urls: Vec<&str> = Vec::new();
    for (i, r) in results.iter().enumerate() {
        out.push_str(&format!("\n[#{}] {}\n", i + 1, r.url));
        match &r.outcome {
            Ok(c) => {
                if !c.title.is_empty() {
                    out.push_str(&format!("标题：{}\n", c.title));
                }
                if !c.body.is_empty() {
                    out.push_str(&format!("正文摘录：{}\n", c.body));
                }
                if c.truncated {
                    out.push_str("（注：响应超过 1MB，已截断，正文可能不完整）\n");
                }
            }
            Err(e) => {
                any_fail = true;
                failed_urls.push(&r.url);
                out.push_str(&format!("抓取失败：{}\n", e));
            }
        }
    }
    if any_fail {
        out.push_str(&format!(
            "\n请在你的回复尾部用单独一行追加：「⚠️ 链接抓取失败：{}」让用户知情。",
            failed_urls.join(" / ")
        ));
    }
    out
}

/// 注入 layer：扫描最后一条 user message → 提 URL → 并发抓 → prepend
/// system note。无 URL / `--no-fetch` / 全失败-空文案 时短路 no-op。
/// async 因为内含 reqwest 调用；与 `inject_persona_layer` 同款 await 风格。
pub async fn inject_url_context_layer(mut messages: Vec<ChatMessage>) -> Vec<ChatMessage> {
    let user_text = match messages.iter().rev().find(|m| m.role == "user") {
        Some(m) => extract_text_from_content(&m.content),
        None => return messages,
    };
    if user_text.contains(NO_FETCH_FLAG) {
        return messages;
    }
    let urls = extract_urls(&user_text);
    if urls.is_empty() {
        return messages;
    }
    let results = fetch_url_summaries(&urls).await;
    let body = format_url_context(&results);
    if body.is_empty() {
        return messages;
    }
    let note: ChatMessage = serde_json::from_value(serde_json::json!({
        "role": "system",
        "content": body,
    }))
    .expect("inject_url_context_layer: JSON shape always parses");
    let insert_at = messages
        .iter()
        .position(|m| m.role != "system")
        .unwrap_or(messages.len());
    messages.insert(insert_at, note);
    messages
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_simple_url() {
        let urls = extract_urls("看这个 https://example.com/foo 不错");
        assert_eq!(urls, vec!["https://example.com/foo"]);
    }

    #[test]
    fn strips_trailing_punctuation() {
        let urls = extract_urls("详见 https://example.com/foo. 还有 https://x.org/bar, 完");
        assert_eq!(
            urls,
            vec!["https://example.com/foo", "https://x.org/bar"]
        );
    }

    #[test]
    fn deduplicates_urls() {
        let urls = extract_urls("a https://x.com b https://x.com c https://y.com");
        assert_eq!(urls, vec!["https://x.com", "https://y.com"]);
    }

    #[test]
    fn caps_at_max_urls() {
        let txt = "https://a.com https://b.com https://c.com https://d.com https://e.com";
        let urls = extract_urls(txt);
        assert_eq!(urls.len(), MAX_URLS);
    }

    #[test]
    fn no_fetch_flag_short_circuits_at_inject_layer_logic() {
        // 单纯校验 NO_FETCH_FLAG 常量值 —— 全链路 async + reqwest 不便单测，
        // 但「--no-fetch」字符串语义保住不被悄悄改名。
        assert_eq!(NO_FETCH_FLAG, "--no-fetch");
    }

    #[test]
    fn extracts_title_from_simple_html() {
        let html =
            "<html><head><title>Hello World</title></head><body><p>Body text</p></body></html>";
        let (title, body) = extract_title_and_body(html);
        assert_eq!(title, "Hello World");
        assert!(body.contains("Body text"));
    }

    #[test]
    fn strips_script_and_style_contents() {
        // script / style 块的内容若直接随 strip-tag 留下，会泄漏一堆 JS / CSS
        // 噪音到 LLM context（既稀释指令、又涨 token）。本 case 防回归。
        let html = "<title>T</title><body>before<script>alert('x')</script>after<style>p{}</style>end</body>";
        let (_, body) = extract_title_and_body(html);
        assert!(!body.contains("alert"));
        assert!(!body.contains("p{}"));
        assert!(body.contains("before"));
        assert!(body.contains("after"));
        assert!(body.contains("end"));
    }

    #[test]
    fn decodes_common_entities() {
        let html = "<title>A &amp; B</title><body>1 &lt; 2&nbsp;always</body>";
        let (title, body) = extract_title_and_body(html);
        assert_eq!(title, "A & B");
        assert_eq!(body, "1 < 2 always");
    }

    #[test]
    fn truncate_chars_appends_ellipsis_only_when_over() {
        assert_eq!(truncate_chars("abc", 10), "abc");
        let t = truncate_chars(&"a".repeat(20), 5);
        assert_eq!(t.chars().count(), 6); // 5 + 「…」
        assert!(t.ends_with('…'));
    }
}
