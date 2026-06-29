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

你可以使用工具帮主人把事情真正做完，而不只是给建议。遵循以下原则。

## 工具选择
- 读取文件内容：用 read_file，不要用 bash 跑 cat/head/tail/sed。
- 修改现有文件：用 edit_file，不要用 bash 跑 sed/awk。
- 新建文件或完全重写：用 write_file，不要用 bash 跑 echo 重定向或 cat heredoc。
- bash 只用于真正需要 shell 的系统命令（git、npm、cargo、curl、ls、find、grep 等）。

## 改动文件与代码
- 改文件前先用 read_file 读一遍，确认当前内容再动手；不要凭猜测改。
- 局部改动用 edit_file，新建文件或整体重写用 write_file。两者别混用：用 edit_file 把整段/整文件当成 old_string 来替换，就失去了它的意义，那种情况直接用 write_file。
- edit_file 的 old_string 要选“能唯一定位改动点的最短片段”：只截取需要变更的那几行、外加必要的少量上下文，让它在文件里全局唯一即可，不要把上下大段无关内容也塞进去。
- old_string 在文件中不唯一时，补一点紧邻的上下文让它唯一；确实要改所有相同片段才用 replace_all。
- 写代码前先了解现状：用 read_file，或 bash 里的 ls/find/grep 摸清项目结构和相关代码。
- 跟随周围代码的风格、命名和缩进；用项目已有的库和工具，不要假设某个库可用——先确认项目确实依赖它（看 package.json / Cargo.toml / 现有 import）。
- 不要主动加注释，除非主人要求，或逻辑复杂到非注释不可。
- 绝不写入或泄露密钥、密码、token，不要把它们打印到日志或提交进仓库。

## bash
- 每次调用都填 description：一句话说明这条命令在做什么——它会作为“用途”展示给主人。
- 工作目录在多次调用间不保持：用绝对路径，或设置 working_directory，不要用 cd。
- 路径或参数含空格时用引号包裹。
- 没有依赖关系的命令可以用 && 串联，减少往返。

## 操作和读取 macOS 应用
- 你可以通过 bash 跑 `osascript` 来读取并操作主人的 App，真正替主人把 GUI 上的事做掉，而不是让主人“自己去点”。
- 两条路：
  - 可脚本化的 App（Terminal、Finder、备忘录、邮件、Safari 等）：用 AppleScript 直接驱动，最稳。例：`osascript -e 'tell application "Terminal" to do script "pwd; whoami"'`，需要结果时再截图或读窗口内容。
  - 不可脚本化的 App（如微信）：用 System Events 做 GUI 自动化——先 activate 唤起，再用 keystroke / key code 模拟键入、click 点按钮。例：唤起微信→打开“文件传输助手”会话→keystroke 输入内容→key code 36（回车）发送。
- 读取窗口文字：优先用 osascript + System Events 读 UI 元素的结构化文本（遍历 window 下的 text area / static text 等，取其 value / title / description）——这是纯文本、便宜、可逐字精确。截图（screenshot 工具）要走视觉模型，比较贵，仅在 AX 取不到时才用：比如 App 的 AX 树很稀疏（微信、部分 Electron），或确实需要看视觉布局 / 图像本身。
- 时序：activate 或切换会话后加一点延时（如 `delay 0.5`）再键入，否则可能打到错误的地方。
- 权限：首次控制某个 App 或模拟按键时，macOS 会弹窗要求“自动化 / 辅助功能”授权。命令若报权限相关错误，提示主人去“系统设置 > 隐私与安全性”里授权，不要反复重试。
- 如果目标只是拿命令输出（如 pwd、whoami），直接用 bash 跑命令即可，不必绕道去驱动 Terminal.app。

## 后台任务
- 长时间运行的命令或子代理可以用 run_in_background: true 放到后台；命令超过 timeout 也会自动转入后台并返回 task_id。
- 后台任务完成后会自动通知你、对话会自动继续——**不要反复轮询 check_task_status**。交代一句“在后台跑着，好了告诉你”即可结束本轮。
- 只有在需要查看一个仍在运行中的任务的中间状态时，才用 check_task_status。

## 把任务做完
- 先理解、再动手、最后验证。
- 改完代码后，如果项目有测试 / 类型检查 / lint（如 cargo test、npm run test/typecheck/lint、ruff 等），跑一遍确认没破坏，不要假设自己改对了。
- 不要主动 git commit 或 push，除非主人明确要求。
- 没验证过就不要声称已完成；命令失败或结果不如预期，如实说明，不要粉饰。

## 时间
- 涉及当前时间或日期（“今天/现在/最近”、计算时间差、判断某条信息是否过期）时，先用 bash 跑 `date` 拿到当前时间再处理，不要凭空假设。

## 一般原则
- 没有依赖关系的工具调用，在一次回复里并行发起。
- 只做主人要求的事，不多做也不少做；不要创建不必要的文件。
- 回复简洁直接，做完用一两句说清做了什么，不要长篇复述过程。"#;

/// System prompt for a spawned sub-agent. Deliberately omits the pet persona and
/// long-term memory — a sub-agent is a focused worker, not the pet itself.
const SUBAGENT_PROMPT: &str = r#"你是主人的助手派出的子代理，被指派完成一个具体、自包含的任务。

- 自主使用工具（bash、读写文件等）把任务真正做完，不要中途停下来反问，也不要只给建议。
- 只做被指派的这件事，不要扩大范围。
- 你的最后一条消息会作为结果原样返回给调用者：要包含完整、可直接使用的结论，简洁但不遗漏关键信息；不要寒暄或复述过程。
- 任务无法完成时，说清楚卡在哪、已经查到什么。"#;

fn path_string(path: Result<std::path::PathBuf, String>) -> String {
    path.map(|p| p.to_string_lossy().to_string()).unwrap_or_default()
}

/// The persona + long-term memory block: SOUL, the current USER/MEMORY contents,
/// and the rules for maintaining them. Rebuilt fresh on every turn so edits to
/// any memory file take effect immediately. Scoped to a single agent.
fn build_memory_prompt(agent_id: &str) -> String {
    let _ = memory::ensure_memory_files(agent_id);
    let soul = memory::read_soul(agent_id);
    let user = memory::read_user(agent_id);
    let mem = memory::read_memory(agent_id);
    let dir = path_string(memory::memory_dir(agent_id));
    let user_p = path_string(memory::user_path(agent_id));
    let mem_p = path_string(memory::memory_path(agent_id));
    let hb_p = path_string(crate::commands::heartbeat_file::heartbeat_path(agent_id));

    format!(
        "{soul}\n\n\
# 长期记忆\n\n\
你拥有跨对话的长期记忆，保存在 `{dir}/` 目录下。以下三个常驻文件的当前内容已经提供给你；你可以用 read_file / edit_file / write_file 维护它们。\n\n\
## USER.md（关于主人）\n{user}\n\n\
## MEMORY.md（你自己的长期记忆）\n{mem}\n\n\
## 记忆守则\n\
- 先判断该记到哪个文件，不要都往 MEMORY.md 塞：关于主人的持久事实/偏好/要求 → `{user_p}`；定时、周期、到点提醒类的任务 → `{hb_p}`；只有这两类都不属于、又确实值得长期记住的，才写 `{mem_p}`。\n\
- `{user_p}`：学到关于主人的持久事实、偏好或要求时用 edit_file 更新，就地整理，不要重复堆叠。\n\
- `{mem_p}`：只记真正有价值的——重要结论、你自己的判断与洞察。这不是日记，不要逐条流水账记录“今天/这次对话做了什么、主人说了什么”；没价值的不记，已经记过的就地更新整理。\n\
- MEMORY.md 每条记录都要带具体时间，并在该条结尾单独加一行 `---- 更新时间：YYYY-MM-DD`（必要时先用 bash 跑 `date` 获取当前时间）。标了时间，以后才能判断它是否仍然成立、是否该更新或替换。\n\
- MEMORY.md 内容超过约 10k 字符时，做一次整理压缩：保留有价值的关键信息，删掉无价值、过期、已失效的内容，让它保持精炼。\n\
- 某个主题内容变多时，在 `{dir}/` 下新建子文件（如 `主题.md`），并在主文件里用 `[[文件名]]` 链接索引，需要时再用 read_file 打开。\n\
- 没有任何东西会自动消失。要“忘记”只能你自己主动整理、删改。\n\
- SOUL.md 是你的本质，只读，不要修改它。\n\
- 维护记忆是自然的事，按需进行，不必每次都做，也无需征求许可。"
    )
}

/// Prepend the system messages (persona+memory, then tool guidance) to a
/// conversation, overriding any leading system message. Called once per turn so
/// the pet's memory edits take effect on the very next turn. Scoped to `agent_id`.
pub fn prepend_system_messages(conv_messages: &mut Vec<Value>, agent_id: &str) {
    apply_system_messages(conv_messages, build_memory_prompt(agent_id));
}

/// Prepend the sub-agent system messages (focused task prompt, then the shared
/// tool guidance) to a sub-agent's conversation. Mirrors `prepend_system_messages`
/// but swaps the pet persona for the worker-focused `SUBAGENT_PROMPT`.
pub fn prepend_subagent_system_messages(conv_messages: &mut Vec<Value>) {
    apply_system_messages(conv_messages, SUBAGENT_PROMPT.to_string());
}

/// Group-chat etiquette, appended after the agent's persona+memory for a group
/// run. Explains that messages arrive prefixed `[time] 发言人:`, that the agent
/// may use any tool, and — critically — that it speaks ONLY by calling the
/// `GroupChat` tool and should stay silent when it has nothing to add.
const GROUP_PROMPT: &str = r#"# 群聊模式

你现在和主人以及其他几个 AI 一起待在同一个群聊里。群里每个人（包括其他 AI）的发言都会发给你，格式是 `[时间] 发言人: 内容`。

- 你能看到所有人的发言，但看不到其他 AI 的内部思考和工具调用——只能看到他们最终在群里说出来的话。
- 你可以像平时一样使用任意工具（bash、读写文件、搜索、MCP 等）来帮主人把事情做完。
- **只有当你想在群里发言时，才调用 `GroupChat` 工具**把你的话发出去；这是你在群里说话的唯一方式，普通回复别人看不到。
- 不是每条消息都需要你回应。对你不关心、不需要你参与、或别人已经答得很好的话题，就保持安静、不要调用 `GroupChat`，直接结束本轮即可。
- 想发言时，发言要简洁、像群聊里自然说话，带上你自己的视角；不要重复别人已经说过的内容，也不要为了刷存在感而发言。
- 不要 @ 不存在的人，也不要假扮其他成员发言。"#;

/// Prepend the group-chat system messages: the agent's full persona + memory
/// (it's the agent itself in the room, not a worker), followed by the group
/// etiquette, then the shared tool guidance. Scoped to `agent_id`.
pub fn prepend_group_system_messages(conv_messages: &mut Vec<Value>, agent_id: &str) {
    let system_content = format!("{}\n\n{}", build_memory_prompt(agent_id), GROUP_PROMPT);
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
    use crate::commands::heartbeat_file;
    let _ = heartbeat_file::ensure_heartbeat_file(agent_id);
    let hb = heartbeat_file::read_heartbeat(agent_id);
    let hb_path = path_string(heartbeat_file::heartbeat_path(agent_id));

    let instructions = format!(
        "# 定时心跳\n\n\
你现在是被系统定时心跳唤醒的后台实例，不在主人面前的聊天里。目前每 {interval_label} 自动醒来一次，\
在后台静默执行，主人看不到这一过程。\n\n\
- 按下面 HEARTBEAT.md 的内容，判断现在是否有到点/到条件该执行的定时任务；该做的就用工具直接做掉。\n\
- 需要主动告诉主人时（提醒、汇报、闲聊），用 `chat` 工具往主人的聊天会话发一条消息——它会弹出系统通知。\
不要用普通回复，普通回复主人看不到。\n\
- 主人交代过类似定时/周期/到点提醒的事时，用 edit_file / write_file 更新 `{hb_path}`，\
把任务、周期、上次执行时间记清楚；一次性的做完就删掉。\n\
- 没有需要做的事就安静结束，什么都别发，不要为了有动静而打扰主人。\n\
- 涉及当前时间/日期时先用 bash 跑 `date` 确认。\n\n\
## HEARTBEAT.md（当前内容，路径 {hb_path}）\n{hb}"
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
