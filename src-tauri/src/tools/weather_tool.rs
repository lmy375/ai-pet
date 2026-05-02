//! Weather tool. Lets the pet glance at the weather to inform small talk ("you working
//! through this rainy afternoon?"). Uses wttr.in's free format=4 endpoint — no API key,
//! one-line response, IP-geolocates if no city is provided.
//!
//! Caveat: wttr.in is best-effort and occasionally rate-limits or returns ASCII art on
//! error; the tool surfaces that text as-is so the LLM can decide whether to use it.

use crate::tools::{Tool, ToolContext};

pub struct GetWeatherTool;

impl Tool for GetWeatherTool {
    fn name(&self) -> &str {
        "get_weather"
    }

    fn definition(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "get_weather",
                "description": "Get a one-line weather snapshot via wttr.in (free, no key required). Useful when proactively talking to the user — weather is great small-talk fuel. Returns text like 'Beijing: 🌦 +18°C'. If city omitted, geolocates by the machine's IP. Don't quote the raw result verbatim; weave it into natural conversation.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "city": {
                            "type": "string",
                            "description": "Optional city name in English or pinyin (e.g. 'Beijing', 'Shanghai', 'Tokyo'). Omit for IP-based location."
                        }
                    },
                    "required": []
                }
            }
        })
    }

    fn execute<'a>(
        &'a self,
        arguments: &'a str,
        ctx: &'a ToolContext,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = String> + Send + 'a>> {
        Box::pin(get_weather_impl(arguments, ctx))
    }
}

async fn get_weather_impl(arguments: &str, ctx: &ToolContext) -> String {
    let args: serde_json::Value = serde_json::from_str(arguments).unwrap_or_default();
    let city = args["city"].as_str().unwrap_or("").trim().to_string();

    let url = if city.is_empty() {
        "https://wttr.in/?format=4".to_string()
    } else {
        // wttr.in URL-encodes spaces with %20 or +; reqwest handles + via path correctly,
        // but city names rarely have spaces. Replace just to be safe.
        let safe = city.replace(' ', "+");
        format!("https://wttr.in/{}?format=4", safe)
    };

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(e) => return format!(r#"{{"error": "build http client: {}"}}"#, e),
    };

    let response = match client.get(&url).send().await {
        Ok(r) => r,
        Err(e) => return format!(r#"{{"error": "wttr.in request failed: {}"}}"#, e),
    };

    let status = response.status();
    let body = match response.text().await {
        Ok(t) => t.trim().to_string(),
        Err(e) => return format!(r#"{{"error": "read body: {}"}}"#, e),
    };

    if !status.is_success() {
        return serde_json::json!({
            "error": format!("wttr.in returned {}", status.as_u16()),
            "body_preview": body.chars().take(200).collect::<String>(),
        })
        .to_string();
    }

    ctx.log(&format!("get_weather: city={:?} -> {:?}", city, body));

    serde_json::json!({
        "weather": body,
        "city": if city.is_empty() { "auto" } else { city.as_str() },
        "source": "wttr.in",
    })
    .to_string()
}
