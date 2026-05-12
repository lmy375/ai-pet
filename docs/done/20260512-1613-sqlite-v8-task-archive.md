# SQLite v8：task_archive 域全套迁移

## 背景

接 v7（todo 已迁完）。本轮迁 **task_archive** —— 归档区，只有 1 write site（consolidate 把 30 天前 done/cancelled 的 butler_tasks 移过来）+ 1 read site（PanelTasks 前端通过 memory_list 加载归档列表）。

## 改动

### 1. Schema（`db.rs::apply_migrations` v3）

新建 `task_archive` 表，与 butler_tasks / todo 同字段集；status 默认 `"archived"`（与 yaml description 里的 `[archived: YYYY-MM-DD]` 头对应）。索引：`idx_task_archive_updated_at`。

### 2. CRUD + helpers

`TaskArchiveRow = ButlerTaskRow`（schema 同形别名），5 fn：
- `task_archive_list / _get / _create / _update / _delete`

辅助：
- `backfill_task_archive`
- `startup_backfill_task_archive`
- `task_archive_as_memory_items`

### 3. Mirror 双写 4 fn

- `mirror_archive_create / update / delete / rename`

### 4. memory_edit / memory_rename 扩展

`commands/memory.rs` 4 处分支（create / update / delete / rename）从 2 ↦ 3 类目 match：
```rust
match category.as_str() {
    "butler_tasks" => db::mirror_butler_*(...),
    "todo" => db::mirror_todo_*(...),
    "task_archive" => db::mirror_archive_*(...),
    _ => {}
}
```

### 5. memory_list / memory_search 覆盖

新增 task_archive 段从 SQLite 取。

### 6. 启动钩子

`lib.rs` 加 `db::startup_backfill_task_archive()`。

### 7. 单测

- `task_archive_crud_roundtrip`
- 修正 `migrations_idempotent`（mig_count 2 → 3 接纳 v3 + 验 task_archive 表存在）

## 验收

- `cargo build --release` ✅
- `cargo test --lib` ✅ 全 **879 通过**
- 启动后：
  - SQLite `task_archive` 表创建并 backfill yaml 现有归档
  - PanelTasks 加载归档列表（通过 memory_list("task_archive")）走 SQLite
  - consolidate `archive_old_butler_tasks` 写归档时通过 memory_edit → mirror_archive_create 同步 db

## SQLite 流水线

- v0–v6 ✅ butler_tasks
- v7 ✅ todo
- v8 ✅ **task_archive**
- v9 ⏳ mood_state
- v10 ⏳ plan_progress

## 完成

- [x] schema + 5 CRUD + helpers + mirror 4 fn
- [x] memory_edit / rename / list / search 扩展
- [x] startup hook
- [x] 1 个 CRUD 单测 + 修正 migration 测试
- [x] 移到 docs/done/
