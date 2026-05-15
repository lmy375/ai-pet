# 归档区「↩ 恢复到队列」

## 背景

PanelTasks 归档区是只读 list（consolidate 自动把 30 天前已结束的 butler_tasks 挪过来）。但归档不该是单向死胡同 —— 用户可能"那条任务想再做一次 / 那条循环还想重新启用"。当前只能手敲新建。

## 改动

### Backend

#### `src-tauri/src/task_queue.rs`：新增 `strip_archive_markers`

剥 `[archived:` / `[archived` / `[done` / `[error` / `[cancelled` / `[result` 全套终态 marker。保留 `[task pri=...]` header、`[every:]` / `[once:]` / `[deadline:]` schedule 前缀、`#tag` —— 让恢复后的任务仍带原本执行节奏 + 标签。

复用既有 `remove_bracketed_segments` + `collapse_whitespace`，单测覆盖两种典型形态（done+result / cancelled / error）。

#### `src-tauri/src/commands/task.rs`：新增 `#[tauri::command] task_unarchive`

流程：
1. `db::task_archive_get` 读条目（v8 read 路径已切 SQLite）
2. `strip_archive_markers` 还原 description 到 pending 形态
3. 脱 `YYYY-MM-DD_` 前缀（严格 10 字符 + 2 个 `-`）拿回原始 title
4. `memory_edit("create", "butler_tasks", new_title, desc, None)` 创建（自动经 v4 mirror 双写 SQLite）
5. `memory_edit("delete", "task_archive", original_title, None, None)` 清归档

detail.md 不带回 —— archive/<file>.md 仍在盘上保留作老笔记；新 butler_tasks 起一份空。

#### `src-tauri/src/lib.rs`：注册 `commands::task::task_unarchive`

### Frontend

`src/components/panel/PanelTasks.tsx`：每条归档 item 标题行右侧加 **↩ 恢复** 按钮：
- onClick → invoke `task_unarchive(title)` → 成功 reload archive + reload queue
- toast 显结果 / 错误
- title 提示 detail.md 不带回（需手动复制）

## 验收

- `cargo build --release` ✅
- `cargo test --lib` ✅ 全 **885 通过**（原 883 + 2 新 archive marker 测试）
- `npx tsc --noEmit` ✅
- 切「归档」tab → 找一条已归档任务 → 点 ↩ 恢复 → 队列 tab 出现 pending 版本，归档 tab 该条消失

## 完成

- [x] strip_archive_markers + 2 单测
- [x] task_unarchive Tauri command
- [x] lib.rs register
- [x] 前端 ↩ 恢复按钮 + reload
- [x] TODO.md 移除
- [x] 移到 docs/done/
