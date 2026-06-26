//! `web_search` — look things up on the live internet.
//!
//! Backed by [Tavily](https://tavily.com), a search API built for LLM agents:
//! it returns clean JSON (`title` / `url` / `content`) plus an optional direct
//! `answer`, so the model gets usable text without us scraping HTML. It needs a
//! Tavily API key (`AppSettings::search_api_key`); without one the tool is never
//! offered to the model (see `ToolRegistry::new`), so `execute` only has to
//! defend against an empty key, not present the model a tool that always fails.

use crate::tools::{Tool, ToolContext};

/// Tavily's search endpoint.
const TAVILY_URL: &str = "https://api.tavily.com/search";

/// Default number of results when the caller doesn't specify, plus the hard cap
/// (keeps the tool result small; the model rarely needs more).
const DEFAULT_RESULTS: u64 = 5;
const MAX_RESULTS: u64 = 10;

pub struct WebSearchTool;

impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }

    fn definition(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "web_search",
                "description": "Search the live internet and get back a ranked list of results (title, URL, and a content snippet), sometimes with a direct answer. Use this whenever you need current information or facts that may be newer than your training data — news, prices, release versions, documentation, whether something exists, etc. The snippets are often enough to answer; cite the URLs when you do.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "The search query, as you'd type it into a search engine."
                        },
                        "max_results": {
                            "type": "integer",
                            "description": "How many results to return (1-10). Defaults to 5.",
                            "minimum": 1,
                            "maximum": 10
                        }
                    },
                    "required": ["query"]
                }
            }
        })
    }

    fn execute<'a>(
        &'a self,
        arguments: &'a str,
        ctx: &'a ToolContext,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = String> + Send + 'a>> {
        Box::pin(web_search_impl(arguments, ctx))
    }
}

async fn web_search_impl(arguments: &str, ctx: &ToolContext) -> String {
    let api_key = ctx.config.search_api_key.trim().to_string();
    if api_key.is_empty() {
        // The tool is gated on a configured key, so this is a safety net only.
        return r#"{"error": "web search is not configured (no Tavily API key)"}"#.to_string();
    }

    let args = super::parse_args(arguments);
    let query = args["query"].as_str().unwrap_or("").trim().to_string();
    if query.is_empty() {
        return r#"{"error": "missing 'query' parameter"}"#.to_string();
    }
    let max = args["max_results"]
        .as_u64()
        .map(|n| n.clamp(1, MAX_RESULTS))
        .unwrap_or(DEFAULT_RESULTS);

    let body = serde_json::json!({
        "api_key": api_key,
        "query": query,
        "max_results": max,
        "search_depth": "basic",
        "include_answer": true,
    });

    let resp = crate::common::http_client()
        .post(TAVILY_URL)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await;

    let resp = match resp {
        Ok(r) => r,
        Err(e) => return serde_json::json!({ "error": format!("search request failed: {}", e) }).to_string(),
    };
    let status = resp.status();
    if !status.is_success() {
        let detail = resp.text().await.unwrap_or_default();
        // 401 here almost always means a bad/expired key — say so plainly.
        let hint = if status.as_u16() == 401 {
            " (check the Tavily API key in Settings)"
        } else {
            ""
        };
        return serde_json::json!({
            "error": format!("search returned HTTP {}{}: {}", status.as_u16(), hint, detail.trim())
        })
        .to_string();
    }

    let json: serde_json::Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => return serde_json::json!({ "error": format!("failed to parse search response: {}", e) }).to_string(),
    };

    format_results(&query, &json)
}

/// Shape Tavily's response into the compact JSON the model sees: the query, an
/// optional `answer`, and a list of `{title, url, snippet}`.
fn format_results(query: &str, json: &serde_json::Value) -> String {
    let results: Vec<serde_json::Value> = json["results"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .map(|r| {
                    serde_json::json!({
                        "title": r["title"].as_str().unwrap_or(""),
                        "url": r["url"].as_str().unwrap_or(""),
                        "snippet": r["content"].as_str().unwrap_or(""),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let mut out = serde_json::Map::new();
    out.insert("query".into(), serde_json::Value::String(query.to_string()));
    // Tavily returns "" (not null) when it has no synthesized answer.
    if let Some(answer) = json["answer"].as_str().filter(|s| !s.trim().is_empty()) {
        out.insert("answer".into(), serde_json::Value::String(answer.to_string()));
    }
    if results.is_empty() {
        out.insert("results".into(), serde_json::Value::Array(vec![]));
        out.insert("note".into(), serde_json::Value::String("no results found".into()));
    } else {
        out.insert("results".into(), serde_json::Value::Array(results));
    }
    serde_json::Value::Object(out).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_results_maps_tavily_shape() {
        let tavily = serde_json::json!({
            "query": "rust language",
            "answer": "Rust is a systems programming language.",
            "results": [
                { "title": "Rust", "url": "https://rust-lang.org/", "content": "A language empowering everyone." },
                { "title": "Docs", "url": "https://doc.rust-lang.org/", "content": "The standard library." }
            ]
        });
        let out: serde_json::Value =
            serde_json::from_str(&format_results("rust language", &tavily)).unwrap();

        assert_eq!(out["query"], "rust language");
        assert_eq!(out["answer"], "Rust is a systems programming language.");
        assert_eq!(out["results"].as_array().unwrap().len(), 2);
        // `content` is renamed to `snippet` for the model.
        assert_eq!(out["results"][0]["snippet"], "A language empowering everyone.");
        assert_eq!(out["results"][1]["url"], "https://doc.rust-lang.org/");
    }

    #[test]
    fn format_results_handles_empty_and_blank_answer() {
        let tavily = serde_json::json!({ "answer": "", "results": [] });
        let out: serde_json::Value = serde_json::from_str(&format_results("q", &tavily)).unwrap();

        assert_eq!(out["results"].as_array().unwrap().len(), 0);
        assert_eq!(out["note"], "no results found");
        // A blank answer is omitted rather than surfaced as an empty string.
        assert!(out.get("answer").is_none());
    }
}
