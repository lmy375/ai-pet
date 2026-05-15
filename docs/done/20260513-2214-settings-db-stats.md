# PanelSettings 显示 SQLite db 状态

## 背景

SQLite migration v0–v12 把 butler_tasks / todo / task_archive / mood / persona_summary / daily_plan 等业务态都搬到了 `pet.db`。owner 没有"看一眼数据规模"的入口 —— 想验证 backfill 工作 / 想估算 disk 用量都要 sqlite3 cli。

## 改动

### Backend：`src-tauri/src/db.rs`

新增 `DbStats` struct + `#[tauri::command] get_db_stats() -> DbStats`：
- `size_bytes`: pet.db 文件字节
- `butler_tasks_count` / `todo_count` / `task_archive_count` / `kv_state_count`: 四张表 row count

文件读盘失败 / 表查询失败 → 0 兜底（不 panic）。

`lib.rs` 注册 `db::get_db_stats`。

### Frontend：`src/components/panel/PanelSettings.tsx`

「本地数据目录」section 加：
- 目录说明文字补 `pet.db` 一行
- 路径 / 按钮下方新增 monospace stats 行：
  - `pet.db {size_bytes_human}` (KB / MB 自动单位)
  - `butler_tasks: N`
  - `todo: N`
  - `task_archive: N`
  - `kv_state: N`

`dbStats === null`（旧 backend / 命令未注册 / 失败）→ 整块不渲染（保持向后兼容）。

## 不做

- 不显示 ai_insights yaml 段 / sessions 数（与 SQLite 无关，独立模块）
- 不加 ↻ 刷新 / 自动 polling（数据规模慢变化，挂载时一次足够；用户切回 tab 自然重 mount）
- 不展示 _migrations 表（基建表，对 owner 无 actionable 信息）

## 验收

- `cargo build --release` ✅
- `npx tsc --noEmit` ✅
- 切「设置」tab 滚到「本地数据目录」section → 路径下面看到 stats 行
- 单位自适应（少量数据 KB，超 1MB 显 MB）

## 完成

- [x] backend DbStats + get_db_stats command + lib.rs register
- [x] frontend state + fetch + render
- [x] 移到 docs/done/
