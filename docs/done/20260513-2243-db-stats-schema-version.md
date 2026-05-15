# DbStats 加 schema_version 字段

## 背景

上一 tick 加了 `get_db_stats` 返 pet.db 大小 + 4 张表行数。但 owner 没法直接看到 SQLite **schema 跑到哪一档**。开发流水线 v0-v12 累加了 4 个 migration（v1 butler_tasks / v2 todo / v3 task_archive / v4 kv_state）。新机器首启动 + migration 完成后 `_migrations` 表 max version 应该等于 4；如果低于 4，说明启动 hook 没跑完，重启可补。这是 SQLite 健康度的关键指标。

## 改动

### Backend：`src-tauri/src/db.rs::DbStats`

加 `pub schema_version: i32` 字段；`get_db_stats` 在同一个 `with_db` 闭包里 query `SELECT COALESCE(MAX(version), 0) FROM _migrations`。读失败 / 0 行 → 0。

### Frontend：`src/components/panel/PanelSettings.tsx`

`DbStats` 类型加 `schema_version: number`。stats 行渲染加一段 `schema v{N}`，title attribute 解释"当前最新 schema = 4 (v9 加 kv_state 起)；低于此值说明 migration 没跑完"。

## 不做

- 不加 schema_version mismatch 警告 banner（用户看到 v4 心智模型已够；UI 上加红 banner 会引发"是不是我数据丢了"焦虑）
- 不在 _migrations 表写入 applied_at 之外的元信息（保持迁移 schema 简单）

## 验收

- `cargo build --release` ✅
- `npx tsc --noEmit` ✅
- 设置面板「本地数据目录」section stats 行多一段 `schema v4`（当前最高 migration）
- hover 看 tooltip 解释

## 完成

- [x] DbStats schema_version backend
- [x] frontend type + render
- [x] 移到 docs/done/
