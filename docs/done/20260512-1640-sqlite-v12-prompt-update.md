# SQLite v12：prompt 引导 LLM 使用专用工具

## 背景

接 v11（butler_task_edit / todo_edit 新工具已注册）。新工具不写进 prompt，LLM 不会自动用 —— 仍习惯调 memory_edit。本轮把 prompt 文案改成引导新工具。

## 改动

### `src-tauri/src/proactive/prompt_assembler.rs`

两处规则文案：

1. **butler_tasks 执行规则**：`memory_edit update 在 butler_tasks 里记录` → `butler_task_edit (action=update)` + 明确 status marker 用法（`[done]` / `[done] [result: ...]` / `[error: 原因]`）

2. **reminders 规则**：`memory_edit delete 把 todo 条目删掉` → `todo_edit (action=delete)`

### `src-tauri/src/commands/chat.rs`

两处主 chat prompt 注入：

1. **设置提醒约定**：`memory_edit create 在 todo 类别下新建` → `todo_edit (action=create)`

2. **任务委托判断（butler_tasks）**：整段重写 —— 所有 `memory_edit create 到 butler_tasks` → `butler_task_edit (action=create)`；保留所有 `[every:] / [once:] / [deadline:]` 前缀语义说明 + butler vs todo 区分说明（也同步从 `todo` / `butler_tasks` 改成 `todo_edit` / `butler_task_edit`）

### 单测修正

`proactive::prompt_tests::reminders_rule_appears_when_hint_present` 原断言 "memory_edit delete" → 现规则使用 `todo_edit`，断言更新。

## 不动

- 不动 `user_profile` 写入路径（仍走 memory_edit，因为 user_profile 是真正的"记忆"，没新工具）
- 不动 `ai_insights/daily_plan` 推进文案（仍走 memory_edit）—— daily_plan 也属于 ai_insights，没专用工具；后续 v13+ 评估
- 不动 `memory_edit` tool 描述本身 —— 它仍能接 butler_tasks / todo，作为 fallback

## 验证

- `cargo build --release` ✅
- `cargo test --lib` ✅ 全 **880 通过**（含修正后的 reminders_rule_appears）
- LLM 在下一次主动开口 / 收到用户消息时，会在 prompt 里看到新工具名 → 倾向调用 butler_task_edit / todo_edit
- 因为新工具内部仍走 memory_edit + mirror 双写，效果与旧路径完全一致；只是 LLM 的"工具选择粒度"变细了

## SQLite GOAL 兑现状态

> memory 只承担"大模型记忆/回想"职责；butler_tasks / todo / task_archive / 计划进度 / mood 等业务态搬出 memory，另建 sqlite 表（复用 memory 字段：title / description / created_at / updated_at / detail_path / tags / status）。**LLM 通过专用工具读写各域，不再共用 memory_edit。**

| GOAL 条款 | 状态 | 实现 |
|----------|------|------|
| 持久化分层（SQLite 表） | ✅ | v0–v10 |
| LLM 通过专用工具读写各域 | ✅ | v11 实现工具 + v12 prompt 引导 |
| 不再共用 memory_edit | 🟡 | 共存过渡期：新工具引导生效，旧 memory_edit 仍接 butler/todo（向后兼容）；占比观测稳定后可撤旧接受 |

## 完成

- [x] proactive prompt 规则文案
- [x] chat prompt 注入文案
- [x] 单测断言更新
- [x] 移到 docs/done/
