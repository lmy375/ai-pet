# SQLite v5a：task_list 读路径切到 SQLite

## 背景

接 v4（双写已生效）。本轮把 PanelTasks 主入口 `task_list` 的 read 路径从 `memory_list("butler_tasks")` 切到 `db::butler_tasks_list`。

这是第一个切到 SQLite 的读 caller，是 v5 大动作的开篇。其它 caller（proactive、task_heartbeat 等）后续 v5b / v5c 单独迁。

## 改动

### `src-tauri/src/db.rs`

1. `startup_backfill_butler_tasks` 改成**同步**执行（去掉 `std::thread::spawn`）。
   - 原因：v5 切读后，task_list 一旦在 backfill 完成前被调用就会读到空 db
   - 几百条 task 的 INSERT < 10ms，启动 hook 阻塞可接受
   - 失败 eprintln 不 panic 保留

2. 新增 `impl ButlerTaskRow { pub fn to_memory_item() -> MemoryItem }`：
   - 把 ButlerTaskRow 形态转回 MemoryItem
   - status / tags 不进入 —— caller 重新从 description 解析（与 yaml 路径同算法），保数据形态一致

### `src-tauri/src/commands/task.rs::task_list`

替换：
```rust
let index = memory::memory_list(Some("butler_tasks".to_string()))?;
let cat = index.categories.get("butler_tasks")?;
let mut views: Vec<TaskView> = cat.items.iter().map(build_task_view).collect();
```

为：
```rust
let rows = crate::db::with_db(crate::db::butler_tasks_list)?;
let mut views: Vec<TaskView> = rows.iter().map(|r| build_task_view(&r.to_memory_item())).collect();
```

排序 / TaskView 派生逻辑不动 —— 同一 description 文本走同一 build_task_view 路径，产物 TaskView identical。

## 不切的读路径（v5b / v5c 单独评估）

- `proactive.rs` ~10 处 `memory_list("butler_tasks")` —— prompt builder，读完算各种 hint
- `task_heartbeat.rs` —— 长任务过期检测
- `task_queue.rs` 内部辅助函数 `find_butler_task` 等
- `commands/task.rs` 内的 `task_retry / task_cancel / task_mark_done / ...` 写动作前的"找一条" —— 这些仍走 `find_butler_task`（yaml），写路径通过 memory_edit + mirror 同步两边
- PanelMemory 前端 butler_tasks 段（仍调 memory_list）

## 验证

- `cargo build --release` ✅
- `cargo test --lib` ✅ 全 876 通过（含 db:: 6 个）
- 启动后：
  - 调用 `task_list` 看到的是 SQLite 行
  - 因为 v3 / v4 保证 yaml ↔ SQLite 已同步，结果与原 yaml 路径一致

## 完成

- [x] startup_backfill 改同步
- [x] ButlerTaskRow::to_memory_item
- [x] task_list 切 SQLite
- [x] 移到 docs/done/
