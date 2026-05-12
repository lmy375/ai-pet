# SQLite v5c：proactive / chat / task_overdue 读路径切到 SQLite

## 背景

接 v5b（find_butler_task 已切）。本轮把剩余的 yaml 读 butler_tasks 站点一并迁。

## 新 helper

`src-tauri/src/db.rs`：
```rust
pub fn butler_tasks_as_memory_items() -> Vec<MemoryItem>
```
- `with_db(butler_tasks_list)` + 错误 → 空 Vec（与 yaml 路径 `unwrap_or_default` 同语义）
- 让 prompt builder / task_heartbeat / 等"想拿一组 task"的 caller 用单行替换原 7-行 yaml index 解构

## 7 处替换

1. **`proactive.rs` urgent_deadline_count**（ToneSnapshot 构建）—— `[deadline:]` 紧迫计数
2. **`proactive.rs::build_butler_deadlines_hint`** —— deadline 紧迫度 hint
3. **`proactive.rs::build_task_heartbeat_hint`** —— 心跳点名 hint
4. **`proactive.rs::build_butler_tasks_hint`** —— 全任务清单 hint
5. **`proactive.rs::build_task_completion_hint`** —— "刚转 done"完成 hint
6. **`commands/chat.rs::inject_deadline_context_layer`** —— chat 注入 deadline system note
7. **`commands/task.rs::task_overdue_count`** —— 任务面板的过期红点

每处旧形态：
```rust
let Ok(index) = memory_list(Some("butler_tasks".to_string())) else { return ... };
let Some(cat) = index.categories.get("butler_tasks") else { return ... };
cat.items.iter().filter/map(...)
```

新形态：
```rust
crate::db::butler_tasks_as_memory_items().iter().filter/map(...)
```

## 仍走 yaml 的读

- `consolidate.rs` 2 处：memory consolidation 周期性整理，跨多 category 读。consolidate 本身就是为读 / 优化 yaml 设计的；切 db 不在本流水线设计范围。
- `db.rs::startup_backfill_butler_tasks`：本就是 yaml → SQLite 的方向，正确。

## 验收

- `cargo build --release` ✅
- `cargo test --lib` ✅ 全 876 通过
- proactive 每次 tick 的 prompt 构造 / chat 起始的 deadline context / 任务面板红点 / `task_list` 列表 / 各 task_* 写命令的"找一条"—— 全部读 SQLite 单一数据源
- 数据一致性：yaml 写入仍同步 mirror 写 SQLite（v4 双写），两边对齐

## SQLite 流水线

- v0 ✅ foundation
- v1 ✅ CRUD
- v2 ✅ backfill
- v3 ✅ Tauri command + 启动 backfill
- v4 ✅ 双写
- v5a ✅ task_list
- v5b ✅ find_butler_task（8 caller）
- v5c ✅ **proactive / chat / overdue（7 caller）**
- v6 ⏳ 删 memory butler_tasks 分支 + 撤双写
- v7+ ⏳ 迁移 todo / task_archive / mood / plan

## 完成

- [x] butler_tasks_as_memory_items 新 helper
- [x] 7 处 read 站点切换
- [x] 移到 docs/done/
