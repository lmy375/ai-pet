# SQLite v5d：consolidate sweeps 读路径切到 SQLite

## 背景

接 v5c（hot read paths 全切）。consolidate.rs 的两个周期 sweep（`sweep_completed_once_butler_tasks` 和 `archive_old_butler_tasks`）仍走 yaml memory_list。本轮把它们也切了 —— 这是 butler_tasks **最后**两处 yaml 读站点。

## 改动

`src-tauri/src/consolidate.rs`：

两处统一模式 `let Ok(index) = memory_list(...) else; let Some(cat) = ... ; cat.items.iter().filter(...)` → 单行 `crate::db::butler_tasks_as_memory_items().iter().filter(...)`：

1. **`sweep_completed_once_butler_tasks`** —— 找 `[once: ...]` done 过 grace 期的任务清扫
2. **`archive_old_butler_tasks`** —— 30 天前已结束的任务挪 task_archive

写动作（memory_edit("delete", "butler_tasks", ...) / memory_edit("create", "task_archive", ...)）不动 —— 仍走 memory_edit 双写路径，yaml ↔ SQLite 同步。task_archive 类目目前还在 yaml（v7 才迁）。

## 残余 yaml butler_tasks 读

只剩 `db.rs::startup_backfill_butler_tasks` —— 方向正确（yaml → SQLite 的回填），按设计保留。

## 验收

- `cargo build --release` ✅
- `cargo test --lib` ✅ 全 876 通过
- 周期 consolidate 跑时（半小时一次 / 启动后）：sweep 与 archive 候选都从 SQLite 找

## SQLite 流水线

- v0 ✅ foundation
- v1 ✅ CRUD
- v2 ✅ backfill
- v3 ✅ Tauri command + 启动 backfill
- v4 ✅ 双写
- v5a ✅ task_list
- v5b ✅ find_butler_task
- v5c ✅ proactive / chat / overdue
- v5d ✅ **consolidate sweeps**
- v6 ⏳ 撤双写：memory_edit on butler_tasks 仅写 SQLite + 移除 yaml butler_tasks index 段
- v7+ ⏳ 迁移 todo / task_archive / mood / plan

至此 butler_tasks 的**全部 read 路径**都由 SQLite 驱动。写路径仍 yaml + SQLite 双写，v6 才考虑撤 yaml。

## 完成

- [x] 2 处 consolidate read 切换
- [x] 移到 docs/done/
