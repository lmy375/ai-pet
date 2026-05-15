# tool_risk: butler_task_edit / todo_edit 显式 case

## 背景

`tool_risk::assess_tool_risk` 决定每条工具调用的风险等级（Low / Medium / High），High 触发 TR3 人工审核 modal。

`memory_edit` 有 per-action 评级（delete = High，create/update = Medium）。但 SQLite v11 加的 `butler_task_edit` / `todo_edit` 在 switch 里没有显式 case → 走 `_ =>` 默认分支（"未分类工具，默认 Medium"）。后果：

- delete butler_task / delete todo 走 Medium → **没触发人工审核**（与 memory_edit delete 不一致）
- reasons 显"未分类工具 'butler_task_edit'"，对 owner 来说是泄漏内部 staging 状态

## 改动

`src-tauri/src/tool_risk.rs`：

### 抽 helper `assess_persisted_edit_action`

把原 memory_edit match 体抽到独立 fn，接收 `tool_name` + `target_label`（"宠物长期记忆" / "管家任务队列" / "用户提醒列表"）—— 三类持久化 edit 工具共享逻辑：
- delete → High + 不可恢复理由 + safe_alternative "用 update 标失效"
- create / update → Medium + 写入理由
- 未知 action → Medium 兜底

### memory_edit / butler_task_edit / todo_edit 三个 case 调 helper

```rust
"memory_edit" => assess_persisted_edit_action(args_json, "memory_edit", "宠物长期记忆", ...),
"butler_task_edit" => assess_persisted_edit_action(args_json, "butler_task_edit", "管家任务队列", ...),
"todo_edit" => assess_persisted_edit_action(args_json, "todo_edit", "用户提醒列表", ...),
```

### 单测

新增 `butler_task_edit_per_action_matches_memory_edit` / `todo_edit_per_action_matches_memory_edit`：验证 create / update / delete 各自的 risk level + 文案中含对应 target_label 关键词。

memory_edit 现有 4 个 test 不动（行为不变 — helper 提取是纯重构）。

## 验收

- `cargo build --release` ✅
- `cargo test --lib` ✅ 全 **891 通过**（原 889 + 2 新）
- delete butler_task / delete todo 现在正确触发 High + 人工审核 modal

## 完成

- [x] 抽 assess_persisted_edit_action helper
- [x] 三个 tool case 改调 helper
- [x] 2 新 unit test
- [x] 移到 docs/done/
