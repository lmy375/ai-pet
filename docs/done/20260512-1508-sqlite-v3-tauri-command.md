# SQLite v3：Tauri command + 启动 backfill

## 背景

接 v2（backfill 函数已存在但没人调用）。本轮把 backfill 挂到 app startup，并暴露第一个 read-only Tauri command 让前端能独立观察 SQLite 状态。

写路径仍走 memory_edit（v4 才双写），所以本轮零功能变化 —— SQLite 是"shadow store"。

## 改动

### `src-tauri/src/db.rs`

新增两条：

1. **`#[tauri::command] pub fn db_butler_tasks_list() -> Result<Vec<ButlerTaskRow>, String>`**
   - 透过 `with_db` 调 `butler_tasks_list`
   - 前端可 invoke 看 SQLite 是否成功填了 backfill 数据，与 `memory_list("butler_tasks")` 对比

2. **`pub fn startup_backfill_butler_tasks()`**
   - 异步线程（不阻塞 Tauri setup）
   - 读 `memory_list("butler_tasks")` → 转 `Vec<MemoryItem>` → 调 `backfill_butler_tasks(conn, &items)`
   - 失败时 eprintln，不 panic 不阻塞 app（read path 仍在 yaml，下次启动再试）
   - 多次启动幂等（v2 已保证）；只在新插入数 > 0 时 log，避免每次启动刷屏

### `src-tauri/src/lib.rs`

- setup hook 末尾加 `db::startup_backfill_butler_tasks();` 调用
- `invoke_handler` 注册 `db::db_butler_tasks_list`

### 不动

- 任何现有 memory_edit / memory_list / task_* 路径
- 前端无变更（本轮纯后端基建）

## 验收

- `cargo build --release` 通过
- `cargo test --lib db::` 全 6 通过
- App 启动后：
  - `~/.config/pet/pet.db` 文件创建
  - butler_tasks 表填充了与 yaml 同步的数据
  - 前端 `invoke("db_butler_tasks_list")` 返与 memory_list 对照的 rows
- 第二次启动 eprintln 不再显示 "inserted N tasks"（noop 路径）

## 下一步（v4）

双写：让 memory_edit 在 butler_tasks category 上同步写 SQLite。需改：
- `memory_edit("create", "butler_tasks", ...)` → 也调 `butler_task_create`
- `memory_edit("update", "butler_tasks", ...)` → 也调 `butler_task_update`
- `memory_edit("delete", "butler_tasks", ...)` → 也调 `butler_task_delete`

写错失败时记 eprintln 但不阻断 memory_edit 主路径（SQLite 是 shadow，主存仍是 yaml）。

## 完成

- [x] db_butler_tasks_list Tauri command
- [x] startup_backfill_butler_tasks 启动钩子
- [x] lib.rs 注册 + 启动调用
- [x] 移到 docs/done/
