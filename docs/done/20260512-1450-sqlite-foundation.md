# SQLite 持久化分层重构 —— 起步：foundation

## 背景

【用户确认】持久化分层重构（GOAL.md 已确认）：

> memory 只承担"大模型记忆/回想"职责；butler_tasks / todo / task_archive / 计划进度 / mood 等业务态搬出 memory，另建 sqlite 表（复用 memory 字段：title / description / created_at / updated_at / detail_path / tags / status）。LLM 通过专用工具读写各域，不再共用 memory_edit。

这是大型架构变更，单次迭代做不完。本迭代只**奠基**：依赖 / db 模块骨架 / 计划文档；**不动**任何现有路径，零迁移。

## 设计决策

### 选 `rusqlite` 而非 `sqlx`
- `rusqlite` 同步 API，与现 `fn memory_list() -> Result<...>` 同步签名匹配；不引入异步污染
- `bundled` feature 编译时打包 SQLite C 源码，无外部依赖
- 表结构简单（每域一张表），不需 sqlx 的 compile-time check

### 单 DB 文件而非每域一文件
- 用户数据目录下 `pet.db` 单文件
- 跨表 JOIN 仍方便（虽然 v1 不打算 JOIN）
- 备份 / 同步语义统一

### 字段映射

每个业务域表共享以下列（与 memory `MemoryItem` 字段保持一致以降低心智迁移成本）：

```sql
CREATE TABLE butler_tasks (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  title TEXT NOT NULL UNIQUE,           -- 业务键
  description TEXT NOT NULL DEFAULT '',  -- raw 文本，含 [done] / [result: ...] / [origin: ...] 等 marker
  status TEXT NOT NULL DEFAULT 'pending', -- pending / done / error / cancelled
  detail_path TEXT,                       -- detail.md 相对路径
  tags_json TEXT NOT NULL DEFAULT '[]',  -- JSON array; SQLite 无 array 类型
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE INDEX idx_butler_tasks_status ON butler_tasks(status);
CREATE INDEX idx_butler_tasks_updated_at ON butler_tasks(updated_at);
```

后续 `todo` / `task_archive` / `mood_state` / `plan_progress` 表用同样字段集，仅业务语义不同。

### 不动现有 memory_edit
- v1：新 db 与旧 memory 文件并存
- v2：双写（先确保读路径切到 db 不丢数据，再切写）
- v3：从 memory 文件回填 db（首次启动 backfill）
- v4：删除 butler_tasks 等 category 的 memory_edit 入口

本迭代仅做 v0：依赖 + 模块 + schema，让后续可在 db.rs 添函数。

## 改动

### `src-tauri/Cargo.toml`
```toml
rusqlite = { version = "0.32", features = ["bundled"] }
```

### `src-tauri/src/db.rs`（新文件）
- `fn db_path() -> PathBuf`：从 user_data_dir 拼 `pet.db`
- `fn open_db() -> Result<Connection, String>`：懒打开 + 应用 schema（migrations table + butler_tasks）
- `static DB: OnceLock<Mutex<Connection>>`：进程内单连接，按需 lazy_init
- `pub fn with_db<F, R>(f: F) -> Result<R, String> where F: FnOnce(&Connection) -> ...`：HOF 暴露连接，保 mutex 不直接外泄

### `src-tauri/src/lib.rs`
- mod db; 引入

不注册任何 Tauri command。本迭代纯 backend foundation。

## 计划文档

下一轮迭代列表（粗略）：

1. **v0**（本轮）：依赖 + db 模块骨架 + schema for butler_tasks
2. **v1**：在 db.rs 加 `butler_task_*` CRUD 函数（list / create / update / delete），但不连 Tauri command
3. **v2**：写 backfill 函数：扫旧 memory_index.yaml 把 butler_tasks 段读出来，逐条 insert 到 sqlite（启动一次性）
4. **v3**：注册新 Tauri command `butler_db_list` 让前端可独立测；前端写一个 hidden debug button 验证
5. **v4**：开始双写 —— `memory_edit("create"/"update"/"delete", "butler_tasks", ...)` 也同步写 db
6. **v5**：把读路径切到 db（proactive.rs / task_queue.rs / task_heartbeat.rs / PanelMemory butler_tasks 段等）
7. **v6**：删 butler_tasks 从 memory_index.yaml；移除 memory_edit 的 butler_tasks 分支
8. **v7-N**：用同样流程依次迁移 todo / task_archive / mood_state / plan_progress

每步都可以独立验证、独立回退。

## 验收

- `cargo build` 通过
- 没有功能变化（zero impact）
- 启动后 `pet.db` 文件被创建并含 butler_tasks 表（手动 sqlite3 命令可验）

## 完成

- [x] 依赖添加
- [x] db.rs 骨架
- [x] lib.rs 引入
- [x] 计划文档（本文件）
- [x] 移到 docs/done/
