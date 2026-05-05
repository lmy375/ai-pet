# Telegram 派单 — 开发计划

> 对应需求（来自 docs/TODO.md「已确认」）：
> Telegram 派单：从 TG 文本即可下发任务，桌面端接单后把结果与产物回传至 TG 会话。

## 目标

让用户在 TG 里像在面板「聊天」里一样自然下发任务，但**省掉确认卡**（TG UI 没有按钮卡片）—— LLM 识别意图后直接 `task_create` 入队。任务执行完毕（标 `[done]` / `[error]` / `[cancelled]`）时，把结果回传到当初下单的那个 TG 聊天。

两道闭环：
1. **下发**：TG → LLM → `task_create` → 写入 `butler_tasks`（带 `[origin:tg:CHAT_ID]` 标记）
2. **回传**：butler_tasks 状态 → 轮询比对 → 状态翻入终态时把"完成 / 失败 / 取消 + 简短描述"发给原 chat

## 非目标

- 不在 TG 里做"确认卡 / inline keyboard" — 用户在 TG 写出任务的瞬间就是确认。
- 不替换 panel 「聊天」的 `propose_task` 路径 — 桌面侧仍走"卡片确认 → task_create"两步。
- 不做"在 TG 里能查看队列、改任务"。后续如果有需求再做。
- 不做跨用户隔离 — 单个 bot 只服务 `allowed_username` 一个授权人，origin 里只记 chat_id（不记 user_id），与 bot 现有授权语义一致。
- 不在 README 里把 origin 内部协议（`[origin:tg:...]`）暴露给用户 — 那是宠物侧约定，UI 隐藏。

## 设计

### 新工具：`task_create`（LLM 侧直接落盘）

与 `propose_task` 互补：
- **propose_task**：emits 提案 → 前端渲染卡片 → 用户点确认 → 调 task_create Tauri 命令。**适用桌面 panel**。
- **task_create**（新）：直接创建 butler_tasks 条目，无确认。**适用 Telegram**（也可被未来其它无 UI 入口复用）。

参数：title / body? / priority / due? / origin?。和 panel 命令同形 + 加 `origin` 字段（`"tg:<chat_id>"` 字符串）。Tool 内部把 origin 拼到 description 末尾作 `[origin:tg:<chat_id>]` 标记。

### Origin 解析（pure helpers，加在 task_queue.rs）

```rust
pub enum TaskOrigin { Tg(i64) }

pub fn parse_task_origin(desc: &str) -> Option<TaskOrigin>
pub fn strip_origin_marker(desc: &str) -> String  // 给 panel body 显示用
```

`[origin:tg:123456789]` 是约定标记。未来要加其它来源（如 webhook）时这里加 variant 即可。

### TG 系统层注入

在 `telegram::bot.rs` 的 chat pipeline 里加一个 inject 层 `inject_telegram_dispatch_layer(messages, chat_id)`：

```
[Telegram dispatch] 你正在通过 Telegram 与主人对话。如果主人请你做一件
具体的、适合放进任务队列的事（"帮我整理…" / "记得明天…" / "这周
末…"），**直接调用 `task_create`**（不要用 `propose_task` —— TG 没有
确认卡 UI）。调用时务必带上 `origin="tg:<chat_id>"`，让任务完成后能把
结果发回这条对话。chat_id = <CHAT_ID>。
```

这条 system note 让 LLM 在 TG 路径里走 task_create，desktop 路径里仍优先 propose_task。两条工具都注册，靠 prompt 引导分流。

### 状态转移轮询器（TG 完成回传）

`telegram::bot.rs` 的 `start` 中除了 dispatcher 还 spawn 一个 **task watcher 任务**。每 60s：

1. `memory_list("butler_tasks")` 拿全量
2. 过滤出 `parse_task_origin(...)` 是 `Tg(chat_id)` 的条目
3. 用静态 `LAST_TG_TASK_STATUS: Mutex<HashMap<String, TaskStatus>>` 跟当前快照比对
4. 状态从非终态翻入终态（done / error / cancelled）→ 发 TG 消息
5. 更新静态快照

进程冷启动时第一轮：填充快照但**不发消息**（不知道哪些"已经发过"，避免重启就把所有已完成任务再轰炸一遍）。

通知文案 templates：
- done：「✅ 「{title}」 已完成」（如果 description 里能解析出"完成摘要"，附一行）
- error：「⚠️ 「{title}」 执行失败：{reason}」
- cancelled：「🚫 「{title}」 已取消：{reason}」

### Bot 句柄共享给 watcher

`TelegramBot::start` 持有 `Bot` 实例；watcher spawn 时 `bot.clone()` 一份就行（`teloxide::Bot` 内部用 Arc 共享 client）。

### Panel 展示

origin 标记在面板任务列表里**隐藏**：把 `strip_origin_marker(body)` 走 `build_task_view` 即可。tag 仍写在持久化 description 里（让轮询器能识别）。

## 阶段划分

| 阶段 | 范围 | 状态 |
| --- | --- | --- |
| **M1** | task_queue 加 origin helpers + 单测；task_create LLM 工具 + 单测 | ✅ 完成（task_queue 9 条新单测、task_create_tool 11 条新单测） |
| **M2** | TG bot：inject layer + watcher + 通知发送 | ✅ 完成（telegram::bot 14 条新单测） |
| **M3** | panel 隐藏 origin marker；收尾（README / TODO / done/） | ✅ 完成 |

## 复用清单

- `task_queue::{TaskHeader, format_task_description, classify_status, TaskStatus}`
- `commands::task::task_create`（不能直接复用 — 它是 Tauri 命令，无法在 LLM 工具里调；但参数校验逻辑可拷贝或抽 helper）
- `commands::memory::memory_edit("create", "butler_tasks", ...)`
- `tools::Tool` trait + `ToolRegistry::new` 注册
- `teloxide::Bot::send_message`

## 待用户裁定的开放问题

1. **轮询间隔 60s 是否合理**：太密会浪费 IO；太稀疏（如 5min）让用户等久。60s 在两者间。本轮先用 60s 看反馈。
2. **失败 / 取消是否也要通知**：当前选「都通知」。让 TG 用户清楚地看到任务每一种终态，不留疑团。如果反馈"通知太碎"再做开关。
3. **重启后如何处理"我离开期间任务完成了"**：当前选"沉默"（首轮不发消息）。代价是用户在 TG 看不到那些"离线期间完成"的任务；好处是不会被冷启动洗版。

## 进度日志

- 2026-05-04 19:00 — 创建本文档；准备进入 M1。
- 2026-05-04 19:50 — M1-M3 一次性合到 main：
  - **M1**：`task_queue.rs` 加 `TaskOrigin` enum + `parse_task_origin` / `append_origin_marker`（幂等 — 已有 origin 则不覆盖）/ `strip_origin_marker`，9 条新单测覆盖正负 chat_id、缺失、解析失败、round-trip、idempotent、显示剥离。新建 `tools/task_create_tool.rs` 实现 `Tool` trait + 纯 `build_description` + `parse_origin_arg`（拒绝非 `tg:` 前缀），11 条单测。注册到 `ToolRegistry::new` + `BUILTIN_TOOL_NAMES`。`tool_risk` / `tool_review_policy` 各加 `task_create` 一档（Medium 等级 + 描述）。
  - **M2**：`telegram::bot.rs` 加：
    - `inject_telegram_dispatch_layer(messages, chat_id)` —— pure，把"用 task_create + origin=tg:CHAT_ID"提示作为 system note 紧跟在 soul 之后注入。
    - `run_task_watcher(bot)` —— 60s 周期扫 butler_tasks，过滤带 `[origin:tg:...]` 的条目，与 `TaskSnapshot` 比对状态翻动；冷启动首轮静默（只填 snapshot）。
    - pure 子函数 `just_finished` / `format_completion_message` 决定何时发、发什么文案。
    - `TelegramBot` 增加 `watcher_handle: Option<JoinHandle<()>>`，`stop()` 时一并 abort。
    - 14 条新单测覆盖：layer 插入位置 / 负数 chat_id / 没 system 时插最前；just_finished 各种状态转移；done/error/cancelled 文案 emoji 与 reason 处理。
  - **M3**：`commands/task.rs::build_task_view` 调 `strip_origin_marker(body)`，让 origin 标记在面板任务列表里隐藏，不污染用户看到的描述。1 条新单测验证 hide。README 加亮点；`docs/TODO.md` 移除条目；本文件移入 `docs/done/`。
  - **整体**：`cargo test --lib` 769/769 通过；`tsc --noEmit` 干净。
- **开放问题答复**：
  - Q1 60s：保留。本轮先看实战 — TG 用户对回传延迟的容忍度。如果反馈"等太久"再缩；不太可能太密因为 TG 不是聊天机器人节奏。
  - Q2 失败/取消都通知：保留。透明度 > 安静。如果用户嫌通知太碎再加 mode 选项（success_only / all）。
  - Q3 重启后沉默：保留。冷启动只填 snapshot 不发消息，是已知 trade-off：用户看不到"离线期间完成的任务"，但不被冷启动洗版更重要。如果未来想补，可在 snapshot 里持久化"上一次推送过的状态"到磁盘。
