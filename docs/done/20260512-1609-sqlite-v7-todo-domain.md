# SQLite v7：todo 域全套迁移（schema + 双写 + 读切换）

## 背景

butler_tasks 域全链路（v0–v6）就位后，按 GOAL 列出的"todo / task_archive / 计划进度 / mood"分头迁。本轮做 **todo**，规模较小（3 个 read site + 1 个 consolidate sweep），一次完成 schema → CRUD → backfill → 双写 → 读切换 → memory_list 覆盖。

## 改动

### 1. Schema（`db.rs::apply_migrations` v2）

新建 `todo` 表，与 butler_tasks 同字段集；status 默认 `"active"`（todo 无 done/error/cancelled 状态机）。索引：`idx_todo_updated_at`。

### 2. CRUD（`db.rs`）

新增 `TodoRow = ButlerTaskRow`（schema 同形，类型别名复用），5 个 fn：
- `todos_list` / `todo_get` / `todo_create` / `todo_update` / `todo_delete`

字段提取复用 `row_to_task`（列名一致）。

### 3. Backfill + 启动钩子

- `backfill_todos(conn, &[MemoryItem])` —— 幂等，skip existing title
- `startup_backfill_todos()` —— 启动同步调用，与 butler 同模式
- `lib.rs` setup hook 末尾加 `db::startup_backfill_todos()`

### 4. Mirror 双写

- `mirror_todo_create / update / delete / rename` —— best-effort eprintln

### 5. `memory_edit` / `memory_rename` 双写分支扩展

`commands/memory.rs` 4 处分支（create/update/delete/rename）从单 `if category == "butler_tasks"` 扩展为 match：
```rust
match category.as_str() {
    "butler_tasks" => crate::db::mirror_butler_*(...),
    "todo" => crate::db::mirror_todo_*(...),
    _ => {}
}
```

### 6. `memory_list` / `memory_search` 覆盖

`commands/memory.rs` 两条 Tauri command 现在覆盖 butler_tasks **和** todo 两段：caller（含 LLM 工具）看到的总是 SQLite 真相。

### 7. Helper `todos_as_memory_items`

`db.rs` 加 `pub fn todos_as_memory_items() -> Vec<MemoryItem>`（与 butler_tasks_as_memory_items 对称）。

### 8. Read site 切换

| 文件 | 函数 | 用途 |
|------|------|------|
| `proactive.rs` | `get_pending_reminders` | 提醒列表（Panel 顶部 chip + prompt） |
| `proactive.rs` | `build_reminders_hint` | 主动开口 prompt 注入 |
| `consolidate.rs` | `sweep_stale_reminders` | 过期提醒清理 |

### 9. 单测

- `todo_crud_roundtrip` —— create/get/update/delete 全链路
- `todo_backfill_skips_existing` —— 幂等性
- 修正 `migrations_idempotent` 断言（mig_count 1 → 2 接纳新 v2）

## 验收

- `cargo build --release` ✅
- `cargo test --lib` ✅ 全 **878 通过**（原 876 + 2 新 todo）
- 启动后：
  - SQLite `todo` 表创建并 backfill yaml 现有条目
  - 调 `get_pending_reminders` / `build_reminders_hint` / `memory_list("todo")` 都读 SQLite
  - LLM 通过 memory_list / memory_search 看到的 todo 段是 SQLite 真相

## SQLite 流水线

- v0–v6 ✅ butler_tasks 全链路
- v7 ✅ **todo 全链路**（精简版，路径同 butler 但范围小）
- v8 ⏳ task_archive
- v9 ⏳ mood_state
- v10 ⏳ plan_progress

## 完成

- [x] todo schema + CRUD + 5 fn
- [x] backfill + startup hook
- [x] mirror 双写 + memory_edit 扩展
- [x] memory_list / memory_search 覆盖
- [x] 3 read site 切换
- [x] 2 个 todo 单测 + migration 测试修正
- [x] 移到 docs/done/
