//! Construction of the system prompt(s) prepended to every chat conversation.
//!
//! Single source of truth for what the pet is told before each turn:
//! 1. its persona + current long-term memory (read from `memory`), and
//! 2. the tool-usage guidance.
//!
//! `memory` owns the files (read/write/paths); `prompts` owns the wording (and
//! lets the owner override it); this module owns how the two are turned into
//! prompt text and assembled into chat messages. Every caller describes its
//! system block with [`SystemPrompt`] and hands it to one `prepend`, so the
//! assembly order lives here rather than in six near-identical functions.

use genai::chat::{ChatMessage, ChatRole};

use crate::memory;
use crate::prompts::{self, PromptKey};

fn path_string(path: Result<std::path::PathBuf, String>) -> String {
    path.map(|p| p.to_string_lossy().to_string()).unwrap_or_default()
}

/// The available-skills list, as a section ready to append to a system message.
/// Empty when no usable skill exists, so an owner without skills sees a prompt
/// unchanged from before the feature. Rebuilt every turn like the memory files —
/// editing a `SKILL.md` takes effect on the very next turn.
fn skills_section() -> String {
    crate::skills::prompt_block(&crate::skills::list_skills())
        .map(|block| format!("\n\n{block}"))
        .unwrap_or_default()
}

/// The persona + long-term memory block: SOUL, the current USER/MEMORY contents,
/// and the rules for maintaining them. Rebuilt fresh on every turn so edits to
/// any memory file take effect immediately. Scoped to a single agent.
fn build_memory_prompt(agent_id: &str) -> String {
    let _ = memory::ensure_memory_files(agent_id);
    let dir = path_string(memory::memory_dir(agent_id));
    let user_p = path_string(memory::user_path(agent_id));
    let mem_p = path_string(memory::memory_path(agent_id));
    let hb_p = path_string(crate::heartbeat_file::heartbeat_path(agent_id));

    let body = prompts::render(
        &prompts::text(PromptKey::Persona),
        &[
            ("name", &crate::settings::agent_name(agent_id)),
            ("soul", &memory::read_soul(agent_id)),
            ("user", &memory::read_user(agent_id)),
            ("memory", &memory::read_memory(agent_id)),
            ("memory_dir", &dir),
            ("user_path", &user_p),
            ("memory_path", &mem_p),
            ("heartbeat_path", &hb_p),
        ],
    );
    format!("{body}{}", skills_section())
}

fn swarm_system_prompt(
    built_in: PromptKey,
    project_name: &str,
    shared_prompt: &str,
    role_prompt: &str,
) -> String {
    let mut system = format!(
        "{}\n\n当前项目：{}",
        prompts::text(built_in),
        project_name.trim()
    );
    if !shared_prompt.trim().is_empty() {
        system.push_str("\n\n## 项目共享上下文\n");
        system.push_str(shared_prompt.trim());
    }
    if !role_prompt.trim().is_empty() {
        system.push_str("\n\n## 项目角色指令\n");
        system.push_str(role_prompt.trim());
    }
    system
}

/// The tool-usage system message: the static guidance plus the working directory
/// as it stands right now. Built per turn (like the memory prompt), so a
/// directory switched in the UI applies from the very next message on.
fn tool_usage_prompt() -> String {
    prompts::render(
        &prompts::text(PromptKey::ToolUsage),
        &[("workdir", &crate::workdir::get_string())],
    )
}

/// Which system block fronts a run. Each variant produces the same text it did
/// when it was its own `prepend_*` function; placement is shared.
pub enum SystemPrompt<'a> {
    /// The main chat: the agent's persona, memory and skills.
    Chat { agent_id: &'a str },
    /// A sub-agent: a task-focused prompt plus the skills list, no persona.
    Subagent,
    /// A group room: the agent's persona, the room metadata and etiquette.
    Group {
        agent_id: &'a str,
        title: &'a str,
        topic: &'a str,
        members: &'a [String],
    },
    /// A heartbeat: the pet waking up, plus the cadence and HEARTBEAT.md.
    Heartbeat { agent_id: &'a str, interval_label: &'a str },
    /// A swarm worker: project instructions only (no persona — concurrent
    /// copies of one agent must never race on USER/MEMORY files).
    SwarmWorker {
        project: &'a str,
        shared: &'a str,
        role: &'a str,
    },
    /// The swarm orchestrator: the agent's own persona, then the project block
    /// appended after the tool guidance.
    SwarmOrchestrator {
        agent_id: &'a str,
        project: &'a str,
        shared: &'a str,
        role: &'a str,
    },
}

impl SystemPrompt<'_> {
    /// The block that replaces (or becomes) the leading system message.
    fn primary(&self) -> String {
        match self {
            Self::Chat { agent_id } => build_memory_prompt(agent_id),
            Self::Subagent => {
                format!("{}{}", prompts::text(PromptKey::Subagent), skills_section())
            }
            Self::Group { agent_id, title, topic, members } => {
                let mut room = String::new();
                if !title.trim().is_empty() {
                    room.push_str(&format!("当前群聊：{}\n", title.trim()));
                }
                if !topic.trim().is_empty() {
                    room.push_str(&format!("群主题：{}\n", topic.trim()));
                }
                let mut roster = vec!["主人".to_string()];
                for name in *members {
                    let name = name.trim();
                    if !name.is_empty() && !roster.iter().any(|existing| existing == name) {
                        roster.push(name.to_string());
                    }
                }
                room.push_str(&format!("群成员：{}\n\n", roster.join("、")));
                format!(
                    "{}\n\n{}{}",
                    build_memory_prompt(agent_id),
                    room,
                    prompts::text(PromptKey::Group)
                )
            }
            Self::Heartbeat { agent_id, interval_label } => {
                use crate::heartbeat_file;
                let _ = heartbeat_file::ensure_heartbeat_file(agent_id);
                let hb = heartbeat_file::read_heartbeat(agent_id);
                let hb_path = path_string(heartbeat_file::heartbeat_path(agent_id));
                let instructions = prompts::render(
                    &prompts::text(PromptKey::Heartbeat),
                    &[
                        ("interval", interval_label),
                        ("heartbeat_path", &hb_path),
                        ("heartbeat", &hb),
                    ],
                );
                format!("{}\n\n{}", build_memory_prompt(agent_id), instructions)
            }
            Self::SwarmWorker { project, shared, role } => {
                swarm_system_prompt(PromptKey::SwarmWorker, project, shared, role)
            }
            Self::SwarmOrchestrator { agent_id, .. } => build_memory_prompt(agent_id),
        }
    }

    /// A second system message appended after the tool guidance, if the block
    /// needs one.
    fn trailing(&self) -> Option<String> {
        match self {
            Self::SwarmOrchestrator { project, shared, role, .. } => Some(
                swarm_system_prompt(PromptKey::SwarmOrchestrator, project, shared, role),
            ),
            _ => None,
        }
    }

    /// Put this run's system block in front of `conv`, rebuilding it from the
    /// current memory files so edits take effect on the very next turn.
    pub fn prepend(&self, conv: &mut Vec<ChatMessage>) {
        place(conv, self.primary(), self.trailing());
    }
}

/// Shape the message list: override a leading system message with `primary`
/// (or insert one if absent), insert the tool-usage system message right after
/// it, then append `trailing` inside the system block if present. Split out so
/// the contract can be unit-tested without reading the memory files.
fn place(conv: &mut Vec<ChatMessage>, primary: String, trailing: Option<String>) {
    let system = |text: &str| ChatMessage::system(text);
    if conv.first().is_some_and(|m| m.role == ChatRole::System) {
        conv[0] = system(&primary);
    } else {
        conv.insert(0, system(&primary));
    }
    conv.insert(1, system(&tool_usage_prompt()));
    if let Some(trailing) = trailing {
        let index = conv
            .iter()
            .take_while(|m| m.role == ChatRole::System)
            .count();
        conv.insert(index, system(&trailing));
    }
}

/// Turn a minute count into a human-readable cadence, e.g. 60 -> "1 小时",
/// 90 -> "1 小时 30 分钟", 45 -> "45 分钟".
pub fn format_interval_label(minutes: u32) -> String {
    if minutes == 0 {
        return "0 分钟".to_string();
    }
    let h = minutes / 60;
    let m = minutes % 60;
    match (h, m) {
        (0, m) => format!("{m} 分钟"),
        (h, 0) => format!("{h} 小时"),
        (h, m) => format!("{h} 小时 {m} 分钟"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// (role, first text) of each message, after rehydrating the stored JSON —
    /// which is what the LLM transport actually sends.
    fn shape(msgs: &[ChatMessage]) -> Vec<(ChatRole, String)> {
        msgs.iter()
            .map(|m| (m.role.clone(), m.content.first_text().unwrap_or_default().to_string()))
            .collect()
    }

    #[test]
    fn overrides_leading_system_and_inserts_tool_prompt() {
        // A session seeded before this turn: leading system message plus history.
        let mut msgs = vec![ChatMessage::system("OLD SOUL"), ChatMessage::user("hi")];
        place(&mut msgs, "MEMORY".to_string(), None);

        // Leading system message is replaced (not duplicated), tool prompt sits
        // right after it, and the conversation is preserved.
        assert_eq!(
            shape(&msgs),
            vec![
                (ChatRole::System, "MEMORY".to_string()),
                (ChatRole::System, tool_usage_prompt()),
                (ChatRole::User, "hi".to_string()),
            ]
        );
    }

    #[test]
    fn inserts_system_messages_when_none_present() {
        let mut msgs = vec![ChatMessage::user("hi")];
        place(&mut msgs, "MEMORY".to_string(), None);

        assert_eq!(
            shape(&msgs),
            vec![
                (ChatRole::System, "MEMORY".to_string()),
                (ChatRole::System, tool_usage_prompt()),
                (ChatRole::User, "hi".to_string()),
            ]
        );
    }

    /// The system block is rebuilt every turn, and `run_agent` strips it off
    /// the conversation it hands back. Those two must compose: replaying a
    /// stored conversation through another turn has to yield the same shape,
    /// not stack up another copy of the tool-usage prompt each time.
    #[test]
    fn system_block_does_not_accumulate_across_turns() {
        let mut msgs = vec![ChatMessage::user("hi")];
        for _ in 0..3 {
            place(&mut msgs, "MEMORY".to_string(), None);
            // What the run returns to callers for storage.
            msgs = msgs
                .into_iter()
                .skip_while(|m| m.role == ChatRole::System)
                .collect();
        }
        place(&mut msgs, "MEMORY".to_string(), None);
        assert_eq!(
            shape(&msgs),
            vec![
                (ChatRole::System, "MEMORY".to_string()),
                (ChatRole::System, tool_usage_prompt()),
                (ChatRole::User, "hi".to_string()),
            ]
        );
    }

    #[test]
    fn swarm_prompt_layers_builtin_shared_and_role_text() {
        let text = swarm_system_prompt(
            PromptKey::SwarmWorker,
            "竞品调研",
            "只使用已核实的来源。",
            "输出 Markdown 表格。",
        );
        assert!(text.contains("当前项目：竞品调研"));
        assert!(text.contains("只使用已核实的来源。"));
        assert!(text.contains("输出 Markdown 表格。"));
        assert!(text.contains("## 项目共享上下文"));
        assert!(text.contains("## 项目角色指令"));
    }

    #[test]
    fn swarm_main_role_stays_inside_system_block() {
        let mut msgs = vec![ChatMessage::system("OLD SOUL"), ChatMessage::user("hi")];
        SystemPrompt::SwarmOrchestrator {
            agent_id: "agent",
            project: "项目",
            shared: "共享",
            role: "角色",
        }
        .prepend(&mut msgs);
        assert_eq!(msgs[0].role, ChatRole::System);
        assert_eq!(msgs[1].role, ChatRole::System);
        assert_eq!(msgs[2].role, ChatRole::System);
        assert_eq!(msgs[3].role, ChatRole::User);
        assert!(shape(&msgs)[2].1.contains("当前项目：项目"));
    }

    #[test]
    fn group_prompt_lists_the_topic_and_member_roster() {
        let mut msgs = vec![ChatMessage::user("hi")];
        SystemPrompt::Group {
            agent_id: "agent",
            title: "产品讨论",
            topic: "确定下个版本的功能优先级",
            members: &["小宠".to_string(), "小智".to_string()],
        }
        .prepend(&mut msgs);
        let system = &shape(&msgs)[0].1;
        assert!(system.contains("当前群聊：产品讨论"));
        assert!(system.contains("群主题：确定下个版本的功能优先级"));
        assert!(system.contains("群成员：主人、小宠、小智"));
    }
}