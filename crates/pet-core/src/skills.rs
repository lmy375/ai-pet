//! Agent Skills: a global directory where each subdirectory is one skill,
//! holding a `SKILL.md` with YAML frontmatter (`name` + `description`) plus
//! whatever else that skill needs (`references/`, `scripts/`, …).
//!
//! Progressive disclosure — the same two-layer idea as `memory`, applied to
//! know-how instead of facts:
//! - **hot**: only name + description + the absolute `SKILL.md` path go into the
//!   system prompt, every turn. A handful of tokens per skill.
//! - **cold**: the body is opened on demand with the existing `read_file` tool.
//!
//! There is deliberately no skills-specific tool: the prompt block IS the whole
//! model-facing integration. On top of it, `is_command` / `expand_command` give
//! both interfaces a `/skill:<slug>` shortcut that expands into a plain user
//! message, so invoking a skill explicitly needs no engine special-casing
//! either.
//!
//! Skills are global (shared by every agent), unlike memory and MCP servers.

use std::path::{Path, PathBuf};

/// One skill discovered at `<skills_dir>/<slug>/SKILL.md`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Skill {
    /// Frontmatter `name`, falling back to the directory name.
    pub name: String,
    /// The directory name — the identifier behind `/skill:<slug>`. Always
    /// filesystem-safe and unique within the skills dir, which the display name
    /// is not.
    pub slug: String,
    /// Frontmatter `description`: what the model matches a task against.
    pub description: String,
    /// Absolute path to `SKILL.md`, handed to the model for `read_file`.
    pub path: String,
    /// Read/parse failure. `Some` = shown in the UI so the owner can fix it, but
    /// excluded from the prompt and not invocable by command.
    pub error: Option<String>,
}

/// Where skills live when `AppSettings::skills_dir` is empty.
pub const DEFAULT_SKILLS_DIR: &str = "~/.agents/skills";

/// Prefix of the per-skill shortcut command (`/skill:<slug> [task]`).
pub const COMMAND_PREFIX: &str = "/skill:";

/// Descriptions are truncated to this many characters. One pathological
/// `SKILL.md` shouldn't be able to dominate every turn's prompt.
const MAX_DESC_CHARS: usize = 1024;

/// Expand a leading `~` / `~/` against the home directory. Only that form —
/// no `$VAR`, no `~user`; anything else is passed through verbatim.
fn expand_tilde(p: &str) -> PathBuf {
    if let Some(home) = dirs::home_dir() {
        if p == "~" {
            return home;
        }
        if let Some(rest) = p.strip_prefix("~/") {
            return home.join(rest);
        }
    }
    PathBuf::from(p)
}

/// Resolve the configured `skills_dir` value: blank falls back to
/// [`DEFAULT_SKILLS_DIR`], and a leading `~` is expanded.
pub fn resolve_skills_dir(configured: &str) -> PathBuf {
    let raw = configured.trim();
    expand_tilde(if raw.is_empty() { DEFAULT_SKILLS_DIR } else { raw })
}

/// The directory currently scanned for skills, per settings.
pub fn skills_dir() -> PathBuf {
    let configured = crate::settings::get_settings().map(|s| s.skills_dir).unwrap_or_default();
    resolve_skills_dir(&configured)
}

/// Create the skills dir and return it — for the "open in file manager" button,
/// which should work before the owner has created any skill. Nothing else ever
/// creates the directory; a missing one simply means "no skills".
pub fn ensure_skills_dir() -> Result<PathBuf, String> {
    let dir = skills_dir();
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create skills dir {}: {e}", dir.display()))?;
    Ok(dir)
}

#[derive(serde::Deserialize)]
struct Frontmatter {
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: String,
}

/// The YAML between the leading `---` fence and the next `---` line. `None` when
/// the file doesn't open with a fence, or the fence is never closed.
fn frontmatter_of(content: &str) -> Option<String> {
    let mut lines = content.trim_start_matches('\u{feff}').lines();
    if lines.next()?.trim_end() != "---" {
        return None;
    }
    let mut yaml = String::new();
    for line in lines {
        if line.trim_end() == "---" {
            return Some(yaml);
        }
        yaml.push_str(line);
        yaml.push('\n');
    }
    None
}

/// Truncate to `max` *characters* (not bytes — descriptions are often Chinese,
/// and byte slicing would panic mid-codepoint).
fn truncate_chars(s: &str, max: usize) -> String {
    match s.char_indices().nth(max) {
        Some((i, _)) => format!("{}…", &s[..i]),
        None => s.to_string(),
    }
}

/// Parse a `SKILL.md` into `(name, description)`. `dir_name` is the fallback
/// name. Pure — no filesystem access, so the format contract is unit-testable.
pub fn parse_skill_md(content: &str, dir_name: &str) -> Result<(String, String), String> {
    let yaml = frontmatter_of(content)
        .ok_or("缺少 YAML frontmatter（文件需以 --- 开头，并以单独一行 --- 结束）")?;
    // NOTE: no `deny_unknown_fields` — real SKILL.md files carry extra keys
    // (`metadata`, `license`, `allowed-tools`, …) and serde must ignore them.
    let fm: Frontmatter =
        serde_yaml::from_str(&yaml).map_err(|e| format!("frontmatter 解析失败: {e}"))?;
    let name = match fm.name.trim() {
        "" => dir_name.to_string(),
        n => n.to_string(),
    };
    let description = fm.description.trim();
    if description.is_empty() {
        return Err("frontmatter 缺少 description（模型靠它判断何时使用该技能）".to_string());
    }
    Ok((name, truncate_chars(description, MAX_DESC_CHARS)))
}

/// Scan one level of `dir` for `<sub>/SKILL.md`. A missing or unreadable
/// directory yields no skills and no error — that's the default state for
/// anyone who doesn't use skills. Subdirectories without a `SKILL.md` are
/// skipped silently; a broken `SKILL.md` is kept with `error` set so the owner
/// can see and fix it.
pub fn list_skills_in(dir: &Path) -> Vec<Skill> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<Skill> = Vec::new();
    for entry in entries.flatten() {
        let sub = entry.path();
        // `is_dir`/`is_file` follow symlinks, so symlinked skills just work.
        if !sub.is_dir() {
            continue;
        }
        let md = sub.join("SKILL.md");
        if !md.is_file() {
            continue;
        }
        let slug = sub.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
        let path = md.to_string_lossy().to_string();
        out.push(match std::fs::read_to_string(&md) {
            Ok(content) => match parse_skill_md(&content, &slug) {
                Ok((name, description)) => Skill { name, slug, description, path, error: None },
                Err(e) => Skill {
                    name: slug.clone(),
                    slug,
                    description: String::new(),
                    path,
                    error: Some(e),
                },
            },
            Err(e) => Skill {
                name: slug.clone(),
                slug,
                description: String::new(),
                path,
                error: Some(format!("读取失败: {e}")),
            },
        });
    }
    // Sorted so the prompt prefix is stable across turns — `read_dir` order is
    // filesystem-dependent, and an unstable prefix defeats prompt caching.
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

/// Scan the configured skills directory.
pub fn list_skills() -> Vec<Skill> {
    list_skills_in(&skills_dir())
}

/// Render the skills section of the system prompt. `None` when nothing usable
/// was found, so an owner without skills gets a byte-for-byte unchanged prompt.
pub fn prompt_block(skills: &[Skill]) -> Option<String> {
    let usable: Vec<&Skill> = skills.iter().filter(|s| s.error.is_none()).collect();
    if usable.is_empty() {
        return None;
    }
    let mut out = String::from(
        "# 技能\n\n\
主人给你准备了一些「技能包」——每个技能是一份写好的操作手册，讲清楚某类任务该怎么做。\
下面只列出名称和用途，正文没有加载。\n\n\
- 接到任务先扫一眼这个清单：命中某条技能的用途时，先用 read_file 打开它的 SKILL.md，\
按里面的说明动手，不要凭印象猜它怎么用。\n\
- SKILL.md 里常会指向同目录下的其他文件（references/、scripts/ 等），需要时同样用 \
read_file 打开、用 bash 执行。\n\
- 没有命中的就正常做事，不要硬套技能。\n\n\
## 可用技能\n",
    );
    for s in usable {
        out.push_str(&format!("\n- **{}**：{}\n  SKILL.md：{}\n", s.name, s.description, s.path));
    }
    Some(out)
}

/// True when `line` is a `/skill:<slug>` invocation. Interfaces use this to
/// route the line as a chat turn instead of rejecting it as an unknown command.
pub fn is_command(line: &str) -> bool {
    line.trim_start().starts_with(COMMAND_PREFIX)
}

/// Expand `/skill:<slug> [任务]` into the user message actually sent to the
/// model. The expansion *is* the user message — it's what gets shown in the
/// transcript and stored in the session, so there's no "display one thing, send
/// another" split to keep in sync.
///
/// `None` when `line` isn't a skill command, or no usable skill matches the
/// slug. The slug is the first whitespace-delimited token and matches
/// case-insensitively; a skill whose directory name contains spaces can't be
/// reached this way (it still works through the prompt list).
pub fn expand_command(line: &str, skills: &[Skill]) -> Option<String> {
    let rest = line.trim_start().strip_prefix(COMMAND_PREFIX)?;
    let (slug, task) = match rest.find(char::is_whitespace) {
        Some(i) => (&rest[..i], rest[i..].trim()),
        None => (rest, ""),
    };
    if slug.is_empty() {
        return None;
    }
    let skill = skills
        .iter()
        .find(|s| s.error.is_none() && s.slug.eq_ignore_ascii_case(slug))?;
    let head = format!(
        "使用技能「{}」：先用 read_file 打开 {}，按里面的说明执行。",
        skill.name, skill.path
    );
    Some(if task.is_empty() { head } else { format!("{head}\n{task}") })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verbatim header of a real skill (`~/.agents/skills/cobo-agentic-wallet`).
    const REAL_HEADER: &str = r#"---
name: cobo-agentic-wallet
metadata:
  version: "1.0.3"
  revision: 1
description: "Create and manage agentic wallets with Cobo. Use for: transfers, contract calls. NOT for fiat payments."
---

## MANDATORY: Load Reference Files Before Acting
"#;

    fn skill(slug: &str, name: &str, err: Option<&str>) -> Skill {
        Skill {
            name: name.to_string(),
            slug: slug.to_string(),
            description: format!("{name} 的用途"),
            path: format!("/skills/{slug}/SKILL.md"),
            error: err.map(String::from),
        }
    }

    #[test]
    fn parses_frontmatter_and_ignores_unknown_keys() {
        // Real files carry `metadata` and colon-bearing quoted descriptions.
        // This breaks the moment someone adds `deny_unknown_fields` or swaps
        // serde_yaml for a naive line parser.
        let (name, desc) = parse_skill_md(REAL_HEADER, "some-dir").unwrap();
        assert_eq!(name, "cobo-agentic-wallet");
        assert!(desc.starts_with("Create and manage agentic wallets with Cobo."));
        assert!(desc.ends_with("NOT for fiat payments."));
    }

    #[test]
    fn name_falls_back_to_directory_name() {
        // The directory is the identity; `name:` is only a nicer label.
        let md = "---\ndescription: 做点什么\n---\n正文";
        assert_eq!(parse_skill_md(md, "my-skill").unwrap().0, "my-skill");
    }

    #[test]
    fn rejects_broken_frontmatter() {
        // No fence, unterminated fence, and a fence without `description` — the
        // last one matters most: without a description the model has nothing to
        // match on, so the skill is worse than useless in the prompt.
        assert!(parse_skill_md("# 没有 frontmatter\n正文", "d").is_err());
        assert!(parse_skill_md("---\nname: x\ndescription: y\n正文", "d").is_err());
        assert!(parse_skill_md("---\nname: x\n---\n正文", "d").is_err());
    }

    #[test]
    fn truncates_overlong_description_on_a_char_boundary() {
        // Chinese descriptions are multi-byte; byte slicing would panic here.
        let long: String = "字".repeat(2000);
        let md = format!("---\nname: x\ndescription: \"{long}\"\n---\n");
        let (_, desc) = parse_skill_md(&md, "d").unwrap();
        assert_eq!(desc.chars().count(), MAX_DESC_CHARS + 1); // + the ellipsis
        assert!(desc.ends_with('…'));
    }

    #[test]
    fn resolve_skills_dir_defaults_and_expands_tilde() {
        let home = dirs::home_dir().expect("home dir");
        assert_eq!(resolve_skills_dir(""), home.join(".agents/skills"));
        assert_eq!(resolve_skills_dir("   "), home.join(".agents/skills"));
        assert_eq!(resolve_skills_dir("~/.claude/skills"), home.join(".claude/skills"));
        assert_eq!(resolve_skills_dir("/opt/skills"), PathBuf::from("/opt/skills"));
    }

    #[test]
    fn prompt_block_lists_only_usable_skills() {
        let good = skill("alpha", "阿尔法", None);
        let broken = skill("beta", "贝塔", Some("frontmatter 解析失败"));
        let block = prompt_block(&[good, broken]).unwrap();
        assert!(block.contains("阿尔法"));
        assert!(block.contains("/skills/alpha/SKILL.md"));
        assert!(!block.contains("贝塔"));

        // No usable skill ⇒ no section at all, so the prompt is unchanged for
        // owners who don't use skills.
        assert!(prompt_block(&[]).is_none());
        assert!(prompt_block(&[skill("beta", "贝塔", Some("坏了"))]).is_none());
    }

    #[test]
    fn expands_skill_command_with_and_without_task() {
        let skills = vec![skill("alpha", "阿尔法", None)];

        let with = expand_command("/skill:alpha 查一下余额", &skills).unwrap();
        assert!(with.contains("阿尔法"));
        assert!(with.contains("/skills/alpha/SKILL.md"));
        assert!(with.ends_with("\n查一下余额"));

        // No task ⇒ no trailing blank line to confuse the model.
        let without = expand_command("/skill:ALPHA", &skills).unwrap();
        assert!(!without.contains('\n'));
    }

    #[test]
    fn unknown_or_broken_slug_is_not_expanded() {
        let skills = vec![skill("alpha", "阿尔法", None), skill("beta", "贝塔", Some("坏了"))];
        assert!(expand_command("/skill:nope 做点事", &skills).is_none());
        assert!(expand_command("/skill:beta 做点事", &skills).is_none()); // broken
        assert!(expand_command("/skill:", &skills).is_none());

        // The colon is what makes it an invocation — a bare `/skill…` word and
        // ordinary messages must not be mistaken for one.
        assert!(is_command("/skill:alpha"));
        assert!(!is_command("/skills"));
        assert!(!is_command("/skill"));
        assert!(!is_command("/help"));
        assert!(!is_command("帮我查一下余额"));
    }
}
