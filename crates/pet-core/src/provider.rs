//! Which LLM wire protocol an agent speaks.
//!
//! genai infers the protocol from the model name (`AdapterKind::from_model`),
//! but that is a static prefix map — `gpt*` → OpenAI, `claude*` → Anthropic,
//! `gemini*` → Gemini, … — and anything unmatched falls back to Ollama. Behind
//! a gateway (litellm) or with renamed/aliased models that guess is routinely
//! wrong, and a wrong adapter means a malformed request, not a graceful
//! degradation. So the protocol is user-selectable per agent
//! (`AgentConfig::provider`), with the empty string meaning "let genai infer".

use genai::adapter::AdapterKind;

/// Provider ids offered in Settings, paired with their display labels, in
/// display order. `""` is auto-detect. The frontend renders this verbatim, so
/// adding a provider here is enough to expose it in the UI.
pub const PROVIDERS: &[(&str, &str)] = &[
    ("", "Auto (detect from model name)"),
    ("openai", "OpenAI — Chat Completions"),
    ("openai_resp", "OpenAI — Responses API"),
    ("anthropic", "Anthropic"),
    ("gemini", "Gemini"),
    ("deepseek", "DeepSeek"),
    ("xai", "xAI / Grok"),
    ("groq", "Groq"),
    ("moonshot", "Moonshot / Kimi"),
    ("minimax", "MiniMax"),
    ("zai", "Z.ai"),
    ("openrouter", "OpenRouter"),
    ("together", "Together"),
    ("fireworks", "Fireworks"),
    ("cohere", "Cohere"),
    ("ollama", "Ollama"),
];

/// Map a configured provider id to a genai `AdapterKind`. An empty (or
/// unrecognized) id falls back to genai's own model-name inference, so a
/// config written before this field existed keeps working.
pub fn kind(provider: &str, model: &str) -> AdapterKind {
    match provider.trim().to_ascii_lowercase().as_str() {
        "openai" => AdapterKind::OpenAI,
        "openai_resp" => AdapterKind::OpenAIResp,
        "anthropic" => AdapterKind::Anthropic,
        "gemini" => AdapterKind::Gemini,
        "deepseek" => AdapterKind::DeepSeek,
        "xai" => AdapterKind::Xai,
        "groq" => AdapterKind::Groq,
        "moonshot" => AdapterKind::Moonshot,
        "minimax" => AdapterKind::MiniMax,
        "zai" => AdapterKind::Zai,
        "openrouter" => AdapterKind::OpenRouter,
        "together" => AdapterKind::Together,
        "fireworks" => AdapterKind::Fireworks,
        "cohere" => AdapterKind::Cohere,
        "ollama" => AdapterKind::Ollama,
        _ => match AdapterKind::from_model(model) {
            // genai returns Ollama for anything its prefix map doesn't
            // recognize — an Ok value, not an error — which would point a
            // cloud agent at localhost. Unknown means "the usual protocol"
            // here, and that has always been OpenAI chat-completions. A real
            // Ollama user selects it explicitly.
            Ok(AdapterKind::Ollama) | Err(_) => AdapterKind::OpenAI,
            Ok(kind) => kind,
        },
    }
}

/// The provider id `kind` resolved to — used by the Settings UI to show what
/// "Auto" actually picked, so a bad guess is visible before it breaks a chat.
pub fn resolved_id(provider: &str, model: &str) -> &'static str {
    let k = kind(provider, model);
    match k {
        AdapterKind::OpenAI => "openai",
        AdapterKind::OpenAIResp => "openai_resp",
        AdapterKind::Anthropic => "anthropic",
        AdapterKind::Gemini => "gemini",
        AdapterKind::DeepSeek => "deepseek",
        AdapterKind::Xai => "xai",
        AdapterKind::Groq => "groq",
        AdapterKind::Moonshot => "moonshot",
        AdapterKind::MiniMax => "minimax",
        AdapterKind::Zai => "zai",
        AdapterKind::OpenRouter => "openrouter",
        AdapterKind::Together => "together",
        AdapterKind::Fireworks => "fireworks",
        AdapterKind::Cohere => "cohere",
        _ => "ollama",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_provider_beats_model_name_inference() {
        // The real configs this was built for: OpenAI-compatible gateways
        // serving models whose names imply a different provider entirely.
        // Inference gets all of these wrong; the explicit id must win.
        assert_eq!(kind("openai", "claude-sonnet-4-6"), AdapterKind::OpenAI);
        assert_eq!(kind("openai", "gemini-3-pro"), AdapterKind::OpenAI);
        assert_eq!(kind("openai", "deepseek-ai/deepseek-v4-pro"), AdapterKind::OpenAI);
        // And the native protocols stay reachable when actually asked for.
        assert_eq!(kind("anthropic", "claude-sonnet-4-6"), AdapterKind::Anthropic);
        assert_eq!(kind("openai_resp", "gpt-5.6"), AdapterKind::OpenAIResp);
    }

    #[test]
    fn auto_falls_back_to_openai_not_ollama() {
        // An unrecognized model name resolves to Ollama inside genai, which
        // would point a cloud agent at localhost. Anything unmatched must land
        // on OpenAI instead — that's the protocol this app has always spoken.
        assert_eq!(kind("", "GPT-5.5"), AdapterKind::OpenAI);
        assert_eq!(kind("", "some-internal-model-name"), AdapterKind::OpenAI);
        // Auto still works where the name genuinely identifies the provider.
        assert_eq!(kind("", "claude-sonnet-4-6"), AdapterKind::Anthropic);
    }

    #[test]
    fn every_offered_provider_maps_to_a_distinct_adapter() {
        // A typo'd id in PROVIDERS would silently fall through to inference,
        // giving the user a dropdown entry that quietly does something else.
        for (id, _) in PROVIDERS.iter().filter(|(id, _)| !id.is_empty()) {
            assert_eq!(resolved_id(id, "unmatched-model-name"), *id, "provider {id} does not round-trip");
        }
    }
}
