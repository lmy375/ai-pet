//! Construction of the system prompt(s) prepended to every chat conversation.
//!
//! Single source of truth for what the pet is told before each turn:
//! 1. its persona + current long-term memory (read from `memory`), and
//! 2. the tool-usage guidance.
//!
//! `memory` owns the files (read/write/paths); this module owns how their
//! contents are turned into prompt text and assembled into chat messages.

use serde_json::{json, Value};

use crate::commands::memory;

/// Tool usage best practices, injected as a second system message.
const TOOL_USAGE_PROMPT: &str = r#"# 工具使用指南

你可以使用以下工具来帮助用户完成任务。请遵循以下原则：

## 工具选择
- 读取文件内容：使用 read_file，**不要**用 bash 运行 cat/head/tail/sed
- 修改现有文件：使用 edit_file，**不要**用 bash 运行 sed/awk
- 创建新文件或完全重写文件：使用 write_file，**不要**用 bash 运行 echo 重定向或 cat heredoc
- bash 工具仅用于真正需要 shell 执行的系统命令（如 git、npm、cargo、curl、ls、find 等）

## 文件操作原则
- 在修改文件之前，先用 read_file 阅读文件内容，确保了解当前状态
- 优先使用 edit_file 修改文件，它只修改需要变更的部分，比 write_file 更安全
- 仅在创建新文件或需要完全重写时使用 write_file
- 使用 edit_file 时，确保 old_string 在文件中是唯一的；如果不唯一，提供更多上下文使其唯一

## bash 使用原则
- 工作目录在多次调用间不会保持，请使用绝对路径或设置 working_directory 参数
- 对于长时间运行的命令，设置合适的 timeout 或使用 run_in_background: true
- 后台命令通过 check_shell_status 轮询结果

## 时间
- 涉及当前时间或日期的事情（如“今天/现在/最近”、计算时间差、判断某条信息是否过期），先用 bash 运行 `date` 获取当前时间，再据此处理，不要凭空假设当前时间。

## 一般原则
- 保持回复简洁直接
- 不要创建不必要的文件
- 不要在未阅读的情况下修改代码
- 一次可以调用多个工具，如果它们之间没有依赖关系"#;

fn path_string(path: Result<std::path::PathBuf, String>) -> String {
    path.map(|p| p.to_string_lossy().to_string()).unwrap_or_default()
}

/// The persona + long-term memory block: SOUL, the current USER/MEMORY contents,
/// and the rules for maintaining them. Rebuilt fresh on every turn so edits to
/// any memory file take effect immediately.
fn build_memory_prompt() -> String {
    let _ = memory::ensure_memory_files();
    let soul = memory::read_soul();
    let user = memory::read_user();
    let mem = memory::read_memory();
    let dir = path_string(memory::memory_dir());
    let user_p = path_string(memory::user_path());
    let mem_p = path_string(memory::memory_path());

    format!(
        "{soul}\n\n\
# 长期记忆\n\n\
你拥有跨对话的长期记忆，保存在 `{dir}/` 目录下。以下三个常驻文件的当前内容已经提供给你；你可以用 read_file / edit_file / write_file 维护它们。\n\n\
## USER.md（关于主人）\n{user}\n\n\
## MEMORY.md（你的日记）\n{mem}\n\n\
## 记忆守则\n\
- 学到关于主人的持久事实、偏好或要求时，用 edit_file 更新 `{user_p}`，就地整理，不要重复堆叠。\n\
- 有自己的理解、想法、想记住的事，写进 `{mem_p}`，像写日记，不要记流水账（不要逐条记“主人今天说了什么”）。\n\
- 记录有时效性的信息时要带上时间（日期，必要时先获取当前时间）：事情会随时间变化，标注时间才能在以后判断它是否仍然成立、是否需要更新或替换。\n\
- 某个主题内容变多时，在 `{dir}/` 下新建子文件（如 `主题.md`），并在主文件里用 `[[文件名]]` 链接索引，需要时再用 read_file 打开。\n\
- 没有任何东西会自动消失。要“忘记”只能你自己主动整理、删改。\n\
- SOUL.md 是你的本质，只读，不要修改它。\n\
- 维护记忆是自然的事，按需进行，不必每次都做，也无需征求许可。"
    )
}

/// Prepend the system messages (persona+memory, then tool guidance) to a
/// conversation, overriding any leading system message. Called once per turn so
/// the pet's memory edits take effect on the very next turn.
pub fn prepend_system_messages(conv_messages: &mut Vec<Value>) {
    apply_system_messages(conv_messages, build_memory_prompt());
}

/// Shape the message list: override a leading system message with `system_content`
/// (or insert one if absent), then insert the tool-usage system message right
/// after it. Split out from `prepend_system_messages` so this contract can be
/// unit-tested without reading the memory files.
fn apply_system_messages(conv_messages: &mut Vec<Value>, system_content: String) {
    if conv_messages
        .first()
        .and_then(|m| m.get("role"))
        .and_then(|r| r.as_str())
        == Some("system")
    {
        conv_messages[0]["content"] = json!(system_content);
    } else {
        conv_messages.insert(0, json!({ "role": "system", "content": system_content }));
    }
    conv_messages.insert(1, json!({ "role": "system", "content": TOOL_USAGE_PROMPT }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overrides_leading_system_and_inserts_tool_prompt() {
        let mut msgs = vec![
            json!({ "role": "system", "content": "OLD SOUL" }),
            json!({ "role": "user", "content": "hi" }),
        ];
        apply_system_messages(&mut msgs, "MEMORY".to_string());

        // Leading system message is replaced (not duplicated), tool prompt sits
        // right after it, and the conversation is preserved.
        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0]["role"], "system");
        assert_eq!(msgs[0]["content"], "MEMORY");
        assert_eq!(msgs[1]["role"], "system");
        assert_eq!(msgs[1]["content"], TOOL_USAGE_PROMPT);
        assert_eq!(msgs[2]["role"], "user");
        assert_eq!(msgs[2]["content"], "hi");
    }

    #[test]
    fn inserts_system_messages_when_none_present() {
        let mut msgs = vec![json!({ "role": "user", "content": "hi" })];
        apply_system_messages(&mut msgs, "MEMORY".to_string());

        assert_eq!(msgs.len(), 3);
        assert_eq!(msgs[0]["content"], "MEMORY");
        assert_eq!(msgs[1]["content"], TOOL_USAGE_PROMPT);
        assert_eq!(msgs[2]["content"], "hi");
    }
}
