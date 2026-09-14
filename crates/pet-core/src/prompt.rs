//! Construction of the system prompt(s) prepended to every chat conversation.
//!
//! Single source of truth for what the pet is told before each turn:
//! 1. its persona + current long-term memory (read from `memory`), and
//! 2. the tool-usage guidance.
//!
//! `memory` owns the files (read/write/paths); `prompts` owns the wording (and
//! lets the owner override it); this module owns how the two are turned into
//! prompt text and assembled into chat messages. The assembly order — which
//! section follows which — stays here deliberately: an owner edits wording, not
//! the shape of the conversation.

use genai::chat::ChatMessage;
use serde_json::Value;

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

/// Prepend the system messages (persona+memory, then tool guidance) to a
/// conversation, overriding any leading system message. Called once per turn so
/// the pet's memory edits take effect on the very next turn. Scoped to `agent_id`.
pub fn prepend_system_messages(conv_messages: &mut Vec<Value>, agent_id: &str) {
    apply_system_messages(conv_messages, build_memory_prompt(agent_id));
}

/// Prepend the sub-agent system messages (focused task prompt, then the shared
/// tool guidance) to a sub-agent's conversation. Mirrors `prepend_system_messages`
/// but swaps the pet persona for the worker-focused sub-agent prompt.
///
/// Skills are included: a sub-agent only ever sees the one task it was handed,
/// so without the list it can't discover that a skill for that task exists.
pub fn prepend_subagent_system_messages(conv_messages: &mut Vec<Value>) {
    let system_content = format!("{}{}", prompts::text(PromptKey::Subagent), skills_section());
    apply_system_messages(conv_messages, system_content);
}

/// Prepend the group-chat system messages: the agent's full persona + memory
/// (it's the agent itself in the room, not a worker), followed by the group
/// etiquette, then the shared tool guidance. Scoped to `agent_id`.
pub fn prepend_group_system_messages(conv_messages: &mut Vec<Value>, agent_id: &str) {
    let system_content =
        format!("{}\n\n{}", build_memory_prompt(agent_id), prompts::text(PromptKey::Group));
    apply_system_messages(conv_messages, system_content);
}

/// Prepend the heartbeat system messages: the full pet persona + memory (a
/// heartbeat is the pet itself waking up, not a worker), followed by the
/// heartbeat instructions and the current `HEARTBEAT.md`, then tool guidance.
/// `interval_label` is a human-readable cadence (e.g. "1 小时").
pub fn prepend_heartbeat_system_messages(
    conv_messages: &mut Vec<Value>,
    agent_id: &str,
    interval_label: &str,
) {
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

    let system_content = format!("{}\n\n{}", build_memory_prompt(agent_id), instructions);
    apply_system_messages(conv_messages, system_content);
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

/// The tool-usage system message: the static guidance plus the working directory
/// as it stands right now. Built per turn (like the memory prompt), so a
/// directory switched in the UI applies from the very next message on.
fn tool_usage_prompt() -> String {
    prompts::render(
        &prompts::text(PromptKey::ToolUsage),
        &[("workdir", &crate::workdir::get_string())],
    )
}

/// Shape the message list: override a leading system message with `system_content`
/// (or insert one if absent), then insert the tool-usage system message right
/// after it. Split out from `prepend_system_messages` so this contract can be
/// unit-tested without reading the memory files.
fn apply_system_messages(conv_messages: &mut Vec<Value>, system_content: String) {
    let system_msg = |text: &str| crate::llm::store_message(&ChatMessage::system(text));
    if crate::llm::is_system_message(conv_messages.first().unwrap_or(&Value::Null)) {
        conv_messages[0] = system_msg(&system_content);
    } else {
        conv_messages.insert(0, system_msg(&system_content));
    }
    conv_messages.insert(1, system_msg(&tool_usage_prompt()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::load_messages;
    use genai::chat::ChatRole;

    /// (role, first text) of each message, after rehydrating the stored JSON —
    /// which is what the LLM transport actually sends.
    fn shape(msgs: &[Value]) -> Vec<(ChatRole, String)> {
        load_messages(msgs)
            .into_iter()
            .map(|m| {
                let text = m.content.first_text().unwrap_or_default().to_string();
                (m.role, text)
            })
            .collect()
    }

    #[test]
    fn overrides_leading_system_and_inserts_tool_prompt() {
        // A session seeded before this turn: leading system message plus history.
        let mut msgs = vec![
            crate::llm::store_message(&ChatMessage::system("OLD SOUL")),
            crate::llm::store_message(&ChatMessage::user("hi")),
        ];
        apply_system_messages(&mut msgs, "MEMORY".to_string());

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
        let mut msgs = vec![crate::llm::store_message(&ChatMessage::user("hi"))];
        apply_system_messages(&mut msgs, "MEMORY".to_string());

        assert_eq!(
            shape(&msgs),
            vec![
                (ChatRole::System, "MEMORY".to_string()),
                (ChatRole::System, tool_usage_prompt()),
                (ChatRole::User, "hi".to_string()),
            ]
        );
    }

    /// The system block is rebuilt every turn, and `run_chat_pipeline` strips it
    /// off the conversation it hands back. Those two must compose: replaying a
    /// stored conversation through another turn has to yield the same shape,
    /// not stack up another copy of the tool-usage prompt each time.
    #[test]
    fn system_block_does_not_accumulate_across_turns() {
        let mut msgs = vec![crate::llm::store_message(&ChatMessage::user("hi"))];
        for _ in 0..3 {
            apply_system_messages(&mut msgs, "MEMORY".to_string());
            // What the pipeline returns to callers for storage.
            msgs = msgs
                .into_iter()
                .skip_while(crate::llm::is_system_message)
                .collect();
        }
        apply_system_messages(&mut msgs, "MEMORY".to_string());
        assert_eq!(
            shape(&msgs),
            vec![
                (ChatRole::System, "MEMORY".to_string()),
                (ChatRole::System, tool_usage_prompt()),
                (ChatRole::User, "hi".to_string()),
            ]
        );
    }
}
