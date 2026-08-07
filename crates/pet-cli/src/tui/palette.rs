//! The `/` command palette: typing `/` lists every command for the current
//! mode with a description; further input filters; ↑/↓ select, Enter/Tab
//! accept. List commands (`/agents`, `/sessions`, `/tasks`, `/members`) open
//! their own interactive picker overlay on accept — switching happens by
//! selecting a row there, not via argument-taking commands.
//!
//! On top of the static commands, every discovered skill contributes a
//! `/skill:<slug>` entry (chat mode only), so a skill can be invoked directly
//! instead of hoping the model picks it out of the prompt list. That listing is
//! also why there's no separate "show me the skills" command — typing `/` is it.

use pet_core::skills::Skill;

use crate::Mode;

pub struct CommandSpec {
    pub name: &'static str,
    pub desc: &'static str,
    /// Placeholder shown in the palette when the command takes an argument;
    /// `None` = argless (accepting it in the palette runs it immediately).
    pub arg: Option<&'static str>,
}

pub const CHAT_COMMANDS: &[CommandSpec] = &[
    CommandSpec { name: "/agents", desc: "选择并切换 Agent", arg: None },
    CommandSpec { name: "/models", desc: "选择并切换模型", arg: None },
    CommandSpec { name: "/sessions", desc: "选择并切换会话", arg: None },
    CommandSpec { name: "/new", desc: "新建会话", arg: None },
    CommandSpec { name: "/tasks", desc: "查看后台任务", arg: None },
    CommandSpec { name: "/group", desc: "进入多 Agent 群聊", arg: None },
    CommandSpec { name: "/help", desc: "帮助", arg: None },
    CommandSpec { name: "/quit", desc: "退出", arg: None },
];

pub const GROUP_COMMANDS: &[CommandSpec] = &[
    CommandSpec { name: "/members", desc: "选择群成员", arg: None },
    CommandSpec { name: "/pause", desc: "暂停所有 Agent", arg: None },
    CommandSpec { name: "/resume", desc: "恢复群聊", arg: None },
    CommandSpec { name: "/reset", desc: "清空群聊记录（保留成员）", arg: None },
    CommandSpec { name: "/history", desc: "回放最近群聊记录", arg: Some("[条数]") },
    CommandSpec { name: "/back", desc: "返回单聊", arg: None },
    CommandSpec { name: "/help", desc: "帮助", arg: None },
    CommandSpec { name: "/quit", desc: "退出", arg: None },
];

pub fn commands(mode: Mode) -> &'static [CommandSpec] {
    match mode {
        Mode::Chat => CHAT_COMMANDS,
        Mode::Group => GROUP_COMMANDS,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PaletteItem {
    /// What the input becomes when this item is accepted.
    pub insert: String,
    /// Left column in the popup.
    pub label: String,
    /// Right column (description / hint).
    pub desc: String,
    /// Accepting with Enter submits immediately (argless commands).
    pub run: bool,
}

/// Compute the palette for the current input, including one `/skill:<slug>`
/// entry per usable skill. Empty result = palette hidden.
pub fn palette_items(mode: Mode, input: &str, skills: &[Skill]) -> Vec<PaletteItem> {
    if !input.starts_with('/') || input.contains(char::is_whitespace) {
        return vec![];
    }
    let mut items: Vec<PaletteItem> = commands(mode)
        .iter()
        .filter(|c| c.name.starts_with(input))
        .map(|c| PaletteItem {
            insert: if c.arg.is_some() { format!("{} ", c.name) } else { c.name.to_string() },
            label: match c.arg {
                Some(arg) => format!("{} {}", c.name, arg),
                None => c.name.to_string(),
            },
            desc: c.desc.to_string(),
            run: c.arg.is_none(),
        })
        .collect();
    if mode == Mode::Chat {
        items.extend(skill_items(input, skills));
    }
    items
}

/// Skill entries. They take an optional task after the slug, so accepting one
/// completes the text (with a trailing space) instead of sending immediately —
/// otherwise Enter would fire a skill invocation with no task attached.
fn skill_items(input: &str, skills: &[Skill]) -> Vec<PaletteItem> {
    skills
        .iter()
        .filter(|s| s.error.is_none())
        .map(|s| (format!("{}{}", pet_core::skills::COMMAND_PREFIX, s.slug), s))
        .filter(|(name, _)| name.starts_with(input))
        .map(|(name, s)| PaletteItem {
            insert: format!("{name} "),
            label: format!("{name} [任务]"),
            desc: format!("{} · {}", s.name, s.description),
            run: false,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skill(slug: &str, err: Option<&str>) -> Skill {
        Skill {
            name: slug.to_uppercase(),
            slug: slug.to_string(),
            description: "用途".to_string(),
            path: format!("/skills/{slug}/SKILL.md"),
            error: err.map(String::from),
        }
    }

    #[test]
    fn slash_alone_lists_every_command_for_the_mode() {
        assert_eq!(palette_items(Mode::Chat, "/", &[]).len(), CHAT_COMMANDS.len());
        assert_eq!(palette_items(Mode::Group, "/", &[]).len(), GROUP_COMMANDS.len());
    }

    #[test]
    fn prefix_filters_and_marks_run_semantics() {
        // /se → /sessions only (argless → runs immediately, opening the picker).
        let items = palette_items(Mode::Chat, "/se", &[]);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].insert, "/sessions");
        assert!(items[0].run);

        // /mo → /models (argless picker command too).
        let items = palette_items(Mode::Chat, "/mo", &[]);
        assert_eq!(items[0].insert, "/models");
        assert!(items[0].run);

        // /history (group) takes an optional argument → trailing space, no run.
        let items = palette_items(Mode::Group, "/hi", &[]);
        assert_eq!(items[0].insert, "/history ");
        assert!(!items[0].run);
    }

    #[test]
    fn every_usable_skill_gets_a_command() {
        let skills = [skill("alpha", None), skill("beta", None), skill("gamma", Some("坏了"))];

        // One entry per usable skill — the broken one is withheld, since
        // invoking it would fail. This listing replaces a separate "show me the
        // skills" command, so it has to be complete.
        let items = palette_items(Mode::Chat, "/skill", &skills);
        let inserts: Vec<&str> = items.iter().map(|i| i.insert.as_str()).collect();
        assert_eq!(inserts, vec!["/skill:alpha ", "/skill:beta "]);

        // Skill entries complete rather than run, so Enter can't fire an
        // invocation before the owner has typed the task.
        let alpha = &items[0];
        assert!(!alpha.run);
        assert!(alpha.desc.contains("ALPHA"));

        // Typing further narrows to a single skill.
        assert_eq!(palette_items(Mode::Chat, "/skill:a", &skills).len(), 1);

        // Group mode has no skill commands (no /skill: dispatch there).
        assert!(palette_items(Mode::Group, "/skill", &skills).is_empty());
    }

    #[test]
    fn group_mode_has_its_own_commands() {
        let items = palette_items(Mode::Group, "/me", &[]);
        assert_eq!(items[0].insert, "/members");
        assert!(items[0].run);
        assert!(palette_items(Mode::Group, "/sessions", &[]).is_empty());
    }

    #[test]
    fn plain_text_and_arguments_hide_the_palette() {
        assert!(palette_items(Mode::Chat, "你好", &[]).is_empty());
        assert!(palette_items(Mode::Chat, "", &[]).is_empty());
        assert!(palette_items(Mode::Group, "/history 5", &[]).is_empty());
    }
}
