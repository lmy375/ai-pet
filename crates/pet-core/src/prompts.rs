//! Every piece of text the model reads that the app authors itself: the system
//! prompts (`prompt.rs` assembles them) and the built-in tool descriptions.
//!
//! Each one ships as a Markdown file inside the crate (`crates/pet-core/prompts/`,
//! compiled in with `include_str!`) and can be overridden by a file of the same
//! name under `<config>/prompts/`:
//!
//! ```text
//! <config>/prompts/<key>.md         # a system prompt (see `PromptKey`)
//! <config>/prompts/tools/<name>.md  # one tool's `description`
//! ```
//!
//! **An override file is only created when the owner edits one.** Nothing is
//! seeded on first run: a materialized copy would fork the prompt forever, and
//! every later improvement to the built-in text would stop reaching that owner.
//! No file = the built-in default, which keeps updating with the app.
//!
//! Prompts are templates with `{{variable}}` holes filled by `render` — the
//! values (persona, memory contents, working directory, …) stay in code, as does
//! the assembly order of the sections. Only the wording is the owner's.

use std::collections::BTreeMap;
use std::path::PathBuf;

/// One overridable system prompt. The variant name is also its file name
/// (`<config>/prompts/<key>.md`) and the id the UI passes back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptKey {
    /// Persona + long-term memory frame, the first system message of a chat.
    Persona,
    /// Tool-usage guidance, the second system message of every run.
    ToolUsage,
    /// Header of the available-skills section (the list itself is generated).
    Skills,
    /// System prompt of a spawned sub-agent (no persona, no memory).
    Subagent,
    /// Group-chat etiquette, appended after the persona in a group run.
    Group,
    /// Heartbeat instructions, appended after the persona in a heartbeat run.
    Heartbeat,
}

impl PromptKey {
    pub const ALL: [PromptKey; 6] = [
        PromptKey::Persona,
        PromptKey::ToolUsage,
        PromptKey::Skills,
        PromptKey::Subagent,
        PromptKey::Group,
        PromptKey::Heartbeat,
    ];

    pub fn key(self) -> &'static str {
        match self {
            PromptKey::Persona => "persona",
            PromptKey::ToolUsage => "tool_usage",
            PromptKey::Skills => "skills",
            PromptKey::Subagent => "subagent",
            PromptKey::Group => "group",
            PromptKey::Heartbeat => "heartbeat",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        PromptKey::ALL.into_iter().find(|k| k.key() == key)
    }

    /// The built-in text, compiled into the binary.
    pub fn default_text(self) -> &'static str {
        match self {
            PromptKey::Persona => include_str!("../prompts/persona.md"),
            PromptKey::ToolUsage => include_str!("../prompts/tool_usage.md"),
            PromptKey::Skills => include_str!("../prompts/skills.md"),
            PromptKey::Subagent => include_str!("../prompts/subagent.md"),
            PromptKey::Group => include_str!("../prompts/group.md"),
            PromptKey::Heartbeat => include_str!("../prompts/heartbeat.md"),
        }
    }

    /// Placeholders an override MUST keep. These are the ones whose absence
    /// silently removes live data from the prompt — drop `{{memory}}` and the
    /// pet simply stops seeing MEMORY.md, with no symptom but getting dumber —
    /// so `save` refuses the edit instead of letting that happen quietly.
    pub fn required_vars(self) -> &'static [&'static str] {
        match self {
            PromptKey::Persona => &["soul", "user", "memory"],
            PromptKey::ToolUsage => &["workdir"],
            PromptKey::Heartbeat => &["heartbeat"],
            PromptKey::Skills | PromptKey::Subagent | PromptKey::Group => &[],
        }
    }

    /// Every placeholder this prompt can use, for the editor's hint line.
    pub fn vars(self) -> &'static [&'static str] {
        match self {
            PromptKey::Persona => &[
                "name",
                "soul",
                "user",
                "memory",
                "memory_dir",
                "user_path",
                "memory_path",
                "heartbeat_path",
            ],
            PromptKey::ToolUsage => &["workdir"],
            PromptKey::Heartbeat => &["interval", "heartbeat_path", "heartbeat"],
            PromptKey::Skills | PromptKey::Subagent | PromptKey::Group => &[],
        }
    }

    /// Where an override for this prompt lives.
    pub fn path(self) -> Result<PathBuf, String> {
        Ok(prompts_dir()?.join(format!("{}.md", self.key())))
    }

    /// True when an override file exists (i.e. the owner has edited this one).
    pub fn is_customized(self) -> bool {
        self.path().map(|p| p.exists()).unwrap_or(false)
    }
}

/// `<config>/prompts/` — where overrides live.
pub fn prompts_dir() -> Result<PathBuf, String> {
    Ok(crate::common::config_dir()?.join("prompts"))
}

/// Create the overrides dir and return it — for the "open in file manager"
/// button, which should work before anything has been overridden.
pub fn ensure_prompts_dir() -> Result<PathBuf, String> {
    let dir = prompts_dir()?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create prompts dir: {e}"))?;
    Ok(dir)
}

/// The text to use for `key`: the override if there is one, else the built-in.
/// Read fresh on every turn, like the memory files — editing a prompt takes
/// effect on the very next message, with no restart.
pub fn text(key: PromptKey) -> String {
    let overridden = key.path().ok().and_then(|p| std::fs::read_to_string(p).ok());
    normalize(overridden.as_deref().unwrap_or_else(|| key.default_text()))
}

/// Trailing whitespace is stripped so a template composes the same whether or
/// not the file ends with a newline — sections are joined by explicit
/// separators in `prompt.rs`, not by whatever the editor left behind.
fn normalize(s: &str) -> String {
    s.trim_end().to_string()
}

/// Save an override, after checking it still carries every required
/// placeholder. Returns the missing ones in the error so the UI can name them.
pub fn save(key: PromptKey, content: &str) -> Result<(), String> {
    let missing = missing_vars(content, key.required_vars());
    if !missing.is_empty() {
        return Err(format!("缺少必需的变量：{}", missing.join("、")));
    }
    crate::common::write_text(&key.path()?, content)
}

/// Drop the override, going back to the built-in text.
pub fn reset(key: PromptKey) -> Result<(), String> {
    remove(&key.path()?)
}

/// Which of `required` placeholders `content` does not contain, as `{{var}}`.
pub fn missing_vars(content: &str, required: &[&str]) -> Vec<String> {
    required
        .iter()
        .filter(|v| !content.contains(&format!("{{{{{v}}}}}")))
        .map(|v| format!("{{{{{v}}}}}"))
        .collect()
}

/// Fill `{{name}}` holes in `template` from `vars`, in a single pass.
///
/// An unknown placeholder is left verbatim rather than dropped: a typo then
/// shows up as `{{memroy}}` in the LLM log instead of silently emptying that
/// part of the prompt. Values are never rescanned, so memory content that
/// happens to contain `{{...}}` can't inject another substitution.
pub fn render(template: &str, vars: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            out.push_str(&rest[start..]);
            return out;
        };
        match vars.iter().find(|(k, _)| *k == after[..end].trim()) {
            Some((_, value)) => out.push_str(value),
            None => out.push_str(&rest[start..start + 2 + end + 2]),
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}

// ---- tool description overrides ----

/// `<config>/prompts/tools/` — one `<tool name>.md` per overridden description.
fn tools_dir() -> Result<PathBuf, String> {
    Ok(prompts_dir()?.join("tools"))
}

/// The override path for a tool. The name is used verbatim as a file name and
/// MCP servers choose their own tool names, so anything that isn't a plain
/// identifier is refused rather than escaping the directory.
fn tool_path(name: &str) -> Result<PathBuf, String> {
    let safe = !name.is_empty()
        && name.len() <= 128
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'));
    if !safe {
        return Err(format!("工具名不能作为文件名：{name}"));
    }
    Ok(tools_dir()?.join(format!("{name}.md")))
}

/// Every overridden tool description, keyed by tool name. One `read_dir` — the
/// registry needs the whole map anyway, and most runs find no directory at all.
pub fn tool_descriptions() -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let Ok(entries) = tools_dir().and_then(|d| {
        std::fs::read_dir(d).map_err(|e| e.to_string())
    }) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let (Some(name), Ok(content)) = (
            path.file_stem().and_then(|n| n.to_str()),
            std::fs::read_to_string(&path),
        ) else {
            continue;
        };
        let content = normalize(&content);
        if !content.is_empty() {
            out.insert(name.to_string(), content);
        }
    }
    out
}

/// One tool's overridden description, or `None` when it uses the built-in one.
pub fn tool_description(name: &str) -> Option<String> {
    let content = tool_path(name).ok().and_then(|p| std::fs::read_to_string(p).ok())?;
    let content = normalize(&content);
    (!content.is_empty()).then_some(content)
}

pub fn save_tool_description(name: &str, content: &str) -> Result<(), String> {
    if content.trim().is_empty() {
        return Err("描述不能为空".to_string());
    }
    crate::common::write_text(&tool_path(name)?, content)
}

pub fn reset_tool_description(name: &str) -> Result<(), String> {
    remove(&tool_path(name)?)
}

/// Delete a file, treating "already gone" as success — resetting a prompt that
/// was never overridden is a no-op, not an error.
fn remove(path: &std::path::Path) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("Failed to delete {}: {e}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_known_vars_and_keeps_unknown_ones() {
        let out = render(
            "dir {{workdir}}, twice {{workdir}}, typo {{workdr}}",
            &[("workdir", "/tmp")],
        );
        assert_eq!(out, "dir /tmp, twice /tmp, typo {{workdr}}");
    }

    #[test]
    fn substituted_values_are_not_rescanned() {
        // MEMORY.md is owner/pet-authored text pasted straight into the prompt.
        // A `{{...}}` inside it must stay literal, not pull in another variable.
        let out = render("{{memory}}", &[("memory", "记得 {{soul}}"), ("soul", "SOUL")]);
        assert_eq!(out, "记得 {{soul}}");
    }

    #[test]
    fn unterminated_placeholder_is_left_alone() {
        assert_eq!(render("a {{b", &[("b", "x")]), "a {{b");
    }

    /// Every shipped default must satisfy its own validation rule — otherwise
    /// "restore default" would produce a prompt the editor refuses to save back.
    #[test]
    fn built_in_defaults_carry_their_required_vars() {
        for key in PromptKey::ALL {
            assert!(
                missing_vars(key.default_text(), key.required_vars()).is_empty(),
                "{} is missing required vars",
                key.key()
            );
        }
    }

    #[test]
    fn missing_required_var_is_reported_as_the_placeholder() {
        let missing = missing_vars("只有 {{soul}}", PromptKey::Persona.required_vars());
        assert_eq!(missing, vec!["{{user}}".to_string(), "{{memory}}".to_string()]);
    }

    #[test]
    fn tool_name_must_be_usable_as_a_file_name() {
        // MCP servers name their own tools; a name with a path separator must
        // not write outside `prompts/tools/`.
        assert!(tool_path("bash").is_ok());
        assert!(tool_path("ELK_Search__3bd8d88d7a").is_ok());
        assert!(tool_path("../../config").is_err());
        assert!(tool_path("a/b").is_err());
        assert!(tool_path("").is_err());
    }
}
