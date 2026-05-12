# SQLite v5b：find_butler_task 切 SQLite

## 背景

接 v5a（task_list 已切）。本轮把 task.rs 内 hot 辅助 `find_butler_task` 切到 SQLite。该函数被 8 个 caller 复用：

- `task_retry`
- `task_cancel`
- `task_mark_done`
- `task_set_priority`
- `task_set_due`
- `task_set_tags`
- `task_save_detail`
- `task_get_detail`

各 caller "拿一条 task → 改 → 写"流程的前半段全部受益。

## 改动

`src-tauri/src/commands/task.rs::find_butler_task`：

旧：
```rust
fn find_butler_task(title: &str) -> Option<memory::MemoryItem> {
    let index = memory::memory_list(Some("butler_tasks".to_string())).ok()?;
    let cat = index.categories.get("butler_tasks")?;
    cat.items.iter().find(|i| i.title == title).cloned()
}
```

新：
```rust
fn find_butler_task(title: &str) -> Option<memory::MemoryItem> {
    crate::db::with_db(|conn| crate::db::butler_task_get(conn, title))
        .ok()
        .flatten()
        .map(|row| row.to_memory_item())
}
```

收益：
- SQLite 走 UNIQUE 索引 O(1) 查询 vs yaml 全扫
- 与 v5a task_list 同一 data source
- caller 拿到的 MemoryItem 形态完全一致，零外部行为差异

## 注意

历史上 yaml 路径 memory_edit 对重名标题有 `_1` 后缀机制让 title 字段本身仍可重复。SQLite UNIQUE 索引会在 backfill / mirror_create 时拒绝第二条同名 row（v2 backfill 的 idempotent skip + v4 mirror 的 create 失败 eprintln）。当前用户数据若有重名，find_butler_task 现在永远返"最早 backfill 那条"而非"最近创建"——历史上 yaml 路径返"最早一条"，行为基本一致。

## 不切的读路径（留 v5c）

- `proactive.rs` 5 处 `memory_list("butler_tasks")` —— prompt builder
- `task_heartbeat.rs` —— 长任务过期检测
- PanelMemory 前端 butler_tasks 段

## 验收

- `cargo build --release` ✅
- `cargo test --lib` ✅ 全 876 通过
- 8 个写命令（retry/cancel/mark_done/set_priority/set_due/set_tags/save_detail/get_detail）现走 SQLite 找 task → memory_edit 写 yaml → mirror 同步回 SQLite。

## 完成

- [x] find_butler_task 切 SQLite
- [x] 移到 docs/done/
