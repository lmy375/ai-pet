# TG bot `/quick <text>` 命令（静默创 P3 task）（iter #333）

## Background

`/task <title>` 创建任务后 reply 是 `✅ 已加到队列「title」(P3) · 用
/tasks 查看，/cancel title 撤回；想调截止时间请回桌面板` — 信息密集
（含 /tasks / /cancel 指引 + 桌面提示），适合首次使用 / 想确认完整状态
的场景。但 owner 想"快速 dump 灵感 / 想法到队列不被长 reply 打扰"时，
反而希望 reply 越短越好。

本迭代加 `/quick <text>` —— 与 /task 同后端 (`memory_edit("create",
"butler_tasks")` + origin marker) 但 reply 极短（仅 `✓ <title>`），priority
始终 P3（不解析 `!!` / `!!!`）。

## Changes

### `src-tauri/src/telegram/commands.rs`

- enum 加 `Quick { text: String }` 变体（与 `Note` / `Reflect` 同结构）
- `name()` → "quick"；`title()` → text 字段
- 解析器：`"quick" => Some(TgCommand::Quick { text: title })`（与 /note
  同模板：所有 arg 当 text 保空格 / 不解析 priority 前缀）
- 新 pure formatter `format_quick_reply(text, save_ok)`：
  - 空 text → usage hint 含"P3" 说明 + "/task" 升级路径
  - Ok(()) → 仅 `✓ <title>`（与 format_task_created_success 反向）
  - Err(msg) → `⚡ 创建失败：<msg>`
- registry zh + en 都加 ("quick", desc)
- format_help_text 全表加 `/quick <text>` 行（/sleep 之后）
- format_help_for_topic 加 "quick" key + 与 /task / /note 交叉引用
- ALL_HELP_TOPICS 加 "quick"
- 两 drift-defense 名单同步加 "quick"

### `src-tauri/src/telegram/bot.rs`

- 加 `TgCommand::Quick { text }` handler arm（在 Sleep arm 之前）：
  - 空 text → formatter usage hint
  - 否则 priority=3 hardcoded + body=空，复用 /task handler 同
    `format_task_description` + `append_origin_marker` + `memory_edit
    ("create")` 路径
  - 成功 / 失败都走 `format_quick_reply` 文案

### Tests（7 个新 unit test）

- parser：text 正常 / 空 / 多余空白 / `!!` 前缀**不**被解析
- formatter：
  - 空 text → usage hint 含 P3 + /task 路径
  - 成功 reply 极短（仅 `✓ <title>`） + 验证不含 /tasks / /cancel 长指引
  - trim 空白
  - 失败 reply 含 error msg

## Key design decisions

- **priority 固定 P3 不解析 `!!` / `!!!`**：/quick 定位"快速 dump 不被打
  扰" — 让 owner 不必想 priority。想精细化 priority 走 /task（/task !!
  写周报 = P5）。两命令按"输入决策成本 / 输出回复长度"分流。
- **`✓ <title>` 单行 ack 而非完全无 reply**：TG bot 必须 ack 命令
  （`format_split_chunks` 路径要求 reply 字符串非空 — 空字符串会导致
  send_message 失败）。极短 ack 是 silent 的 best approximation 且让
  owner 看到"command 收到了"。
- **复用 /task 后端而非独写**：100% 走 `memory_edit("create",
  "butler_tasks")` 同入口 — origin marker / 桌面 watcher 通知 / SQLite
  mirror 等所有副作用都自动跟进。/quick 是 frontend 语法糖，不是新
  semantic。
- **formatter pure**：与既有 format_note_reply / format_reflect_reply
  同 pattern — caller 传 Result，单测稳定。
- **不解析 `!!` prefix**：让 `/quick !! something` 创建 title=`!! something`
  (P3)。owner 想"!! P5 提示"走 /task 即可 — /quick 名字暗示了"不要做
  priority parsing"。
- **registry desc 用英文 / 中文双轨**：与既有 28 条命令注册保持一致 —
  TG slash autocomplete owner 可按界面语言看到本地化描述。

## Verification

- `cargo test --lib`（backend）— 1208 passed / 0 failed（7 新 quick 测
  试通过；两 drift-defense 也命中新加的 "quick"）
- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.24s)
