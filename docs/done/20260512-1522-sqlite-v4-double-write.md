# SQLite v4：butler_tasks 双写

## 背景

接 v3（startup backfill + read-only command 已通）。本轮让 memory_edit 在 butler_tasks category 上写 yaml 之后**同步**写 SQLite，保两边数据同步。

写错失败 eprintln 不阻断 —— yaml 仍是 source of truth，db 是 shadow。

## 改动

### `src-tauri/src/db.rs` 新增 4 个 mirror 函数

best-effort，失败 eprintln：

- `mirror_butler_create(item: &MemoryItem)` —— 用 v2 同算法派生 status / tags / detail_path，调 `butler_task_create`
- `mirror_butler_update(item: &MemoryItem)` —— 调 `butler_task_update`；命中 0 行（title 不在 db）时 fallback 到 `butler_task_create` 让两边对齐（防 backfill 时漏掉的 case）
- `mirror_butler_delete(title: &str)` —— 调 `butler_task_delete`
- `mirror_butler_rename(old_title, new_item: &MemoryItem)` —— SQLite UNIQUE title 不可直接 ALTER；用 delete-old + create-new

### `src-tauri/src/commands/memory.rs`

3 处 `memory_edit` 分支 + `memory_rename` 末尾加 mirror 调用（仅 `category == "butler_tasks"`）：

- `"create"` 分支：push 到 yaml + write_index 后，clone item 调 `mirror_butler_create`
- `"update"` 分支：write_index 前 snapshot（避免再借 index），write 完调 `mirror_butler_update`
- `"delete"` 分支：snapshot removed_title，write 完调 `mirror_butler_delete`
- `memory_rename`：write_index 前 snapshot 新 item，write 完调 `mirror_butler_rename`

## 不做

- 不动其它 category（ai_insights / todo / user_profile / general / task_archive）的写路径 —— 它们暂不在 SQLite
- 不暴露 db_butler_task_create / update / delete 的 Tauri command —— 前端继续走 memory_edit，db 写由后端 mirror 触发
- 不写新的端到端集成测试 —— mirror_* 函数内部调已测的 CRUD；mirror 行为正确性靠人工验证（启动后 panel 改一条任务 → 观察 SQLite）

## 验收

- `cargo build --release` 通过
- `cargo test --lib db::` 全 6 通过（v0–v2 单测不动）
- 人工验证（v5 切读路径前的 sanity check）：
  - 创建任务 → SQLite butler_tasks 表有新行
  - 改任务（标 done / 改 desc）→ 该行 description / status 更新
  - 删任务 → 该行消失
  - 改名任务 → 旧 title 行消失、新 title 行出现

## 下一步（v5）

读路径切到 SQLite：
- `proactive.rs` 内多处 `memory_list(Some("butler_tasks"...))` → 改 `db_butler_tasks_list` 等
- `task_queue.rs` / `task_heartbeat.rs` 同
- `PanelMemory` 前端 butler_tasks 段 → 走 db_butler_tasks_list
- `PanelTasks` task list（虽然走的是 task_list 命令，但 task_list 内部也读 memory）

那是 v5 的大动作。本轮 v4 仅写路径双写，让后续切读时数据已同步好。

## 完成

- [x] 4 个 mirror 函数
- [x] memory_edit 3 分支 + memory_rename hook
- [x] 移到 docs/done/
