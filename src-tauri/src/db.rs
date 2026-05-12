//! SQLite 持久化层（GOAL: butler_tasks / todo / task_archive / mood / plan
//! 等业务态搬出 memory）。本文件是 v0 foundation：lazy-init 单连接 +
//! migration table + butler_tasks 表 schema。**不动**现有 memory_edit
//! 路径；现读写仍走 yaml index。后续迭代逐步切。
//!
//! 路径：`~/.config/pet/pet.db`（与 memories/ 同 parent dir）
//!
//! 设计要点：
//! - 单文件 DB（跨表 JOIN 留余地 + 备份语义统一）
//! - 同步 rusqlite + bundled feature（编译时静态打包 SQLite C，无外部依赖）
//! - 进程内单连接 + Mutex 串行化（SQLite 支持多线程但 Connection 不 Sync）
//! - 每域表共享 (title / description / status / detail_path / tags_json /
//!   created_at / updated_at) 字段集，与 MemoryItem 对齐降心智迁移成本

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

/// butler_tasks 表的 row 投影。与前端期望 + memory MemoryItem 对齐，新增
/// 显式 status / tags 字段（旧 MemoryItem 是从 description 文本里 parse 的，
/// SQLite 让我们顺便把它们升级成正经列）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ButlerTaskRow {
    pub title: String,
    pub description: String,
    pub status: String,
    pub detail_path: Option<String>,
    pub tags: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

fn now_iso() -> String {
    chrono::Local::now()
        .format("%Y-%m-%dT%H:%M:%S%:z")
        .to_string()
}

impl ButlerTaskRow {
    /// 转回 MemoryItem 形态。让 `task_queue::build_task_view` /
    /// proactive prompt builder 等 caller 继续按既有形态拿数据。
    /// status / tags 不进入 MemoryItem —— caller 重新从 description 解析
    /// （与 yaml 路径同算法），保数据形态一致。
    pub fn to_memory_item(&self) -> crate::commands::memory::MemoryItem {
        crate::commands::memory::MemoryItem {
            title: self.title.clone(),
            description: self.description.clone(),
            detail_path: self.detail_path.clone().unwrap_or_default(),
            created_at: self.created_at.clone(),
            updated_at: self.updated_at.clone(),
        }
    }
}

fn row_to_task(row: &rusqlite::Row<'_>) -> rusqlite::Result<ButlerTaskRow> {
    let tags_json: String = row.get("tags_json")?;
    let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
    Ok(ButlerTaskRow {
        title: row.get("title")?,
        description: row.get("description")?,
        status: row.get("status")?,
        detail_path: row.get("detail_path")?,
        tags,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

/// `~/.config/pet/pet.db`。memories_dir 的 parent。
fn db_path() -> Result<PathBuf, String> {
    let dir = dirs::config_dir()
        .ok_or_else(|| "Cannot determine config directory".to_string())?
        .join("pet");
    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create config dir: {e}"))?;
    Ok(dir.join("pet.db"))
}

/// 全局单连接（lazy）。`OnceLock<Mutex<Connection>>` 保进程内只一次
/// open，且对外接口通过 mutex 串行化（与 rusqlite Connection 非 Sync 一致）。
static DB: OnceLock<Mutex<Connection>> = OnceLock::new();

/// Lazy 初始化：首次访问时打开 DB + 跑迁移。后续访问直接 lock 现连接。
/// 失败时 panic —— DB 不可用则任何依赖它的功能都跑不起来，让早期硬失败
/// 比静默走 fallback 路径更安全（避免数据写错地方）。
fn init_db() -> Mutex<Connection> {
    let path = db_path().expect("db_path resolution must succeed");
    let conn = Connection::open(&path).unwrap_or_else(|e| {
        panic!("failed to open SQLite at {path:?}: {e}");
    });
    apply_migrations(&conn).unwrap_or_else(|e| {
        panic!("failed to apply DB migrations: {e}");
    });
    Mutex::new(conn)
}

/// 执行闭包；自动 lock 共享连接。错误透传，不吞。
pub fn with_db<F, R>(f: F) -> Result<R, String>
where
    F: FnOnce(&Connection) -> Result<R, rusqlite::Error>,
{
    let db = DB.get_or_init(init_db);
    let guard = db.lock().map_err(|e| format!("db mutex poisoned: {e}"))?;
    f(&guard).map_err(|e| format!("db error: {e}"))
}

/// migration table 自管：每条 migration 一行 (version + applied_at)。新建
/// migration 时往下加分支即可，不必维护单独的 migration registry crate。
fn apply_migrations(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS _migrations (
            version INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL
        );",
    )?;
    let current: i32 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM _migrations",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);
    // v1：butler_tasks 表。与 memory MemoryItem 字段对齐。tags 用 JSON
    // 文本（SQLite 无 array），caller serde_json 序列化。
    if current < 1 {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS butler_tasks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                title TEXT NOT NULL UNIQUE,
                description TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL DEFAULT 'pending',
                detail_path TEXT,
                tags_json TEXT NOT NULL DEFAULT '[]',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_butler_tasks_status ON butler_tasks(status);
            CREATE INDEX IF NOT EXISTS idx_butler_tasks_updated_at ON butler_tasks(updated_at);
            INSERT INTO _migrations (version, applied_at) VALUES (1, datetime('now'));",
        )?;
    }
    // v2：todo 表。schema 与 butler_tasks 同形（共享字段集）；status 默
    // 认 "active"（todo 没有 done / error 状态机，只有"在 / 不在"）。
    if current < 2 {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS todo (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                title TEXT NOT NULL UNIQUE,
                description TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL DEFAULT 'active',
                detail_path TEXT,
                tags_json TEXT NOT NULL DEFAULT '[]',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_todo_updated_at ON todo(updated_at);
            INSERT INTO _migrations (version, applied_at) VALUES (2, datetime('now'));",
        )?;
    }
    // v3：task_archive 表。归档区只追加不修改，schema 同形；status 默认
    // "archived"（与 yaml description 里的 `[archived: YYYY-MM-DD]` 头
    // 对应）。归档量随时间增长，加 idx_task_archive_updated_at 让"看最
    // 近 N 条"查询快。
    if current < 3 {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS task_archive (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                title TEXT NOT NULL UNIQUE,
                description TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL DEFAULT 'archived',
                detail_path TEXT,
                tags_json TEXT NOT NULL DEFAULT '[]',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_task_archive_updated_at ON task_archive(updated_at);
            INSERT INTO _migrations (version, applied_at) VALUES (3, datetime('now'));",
        )?;
    }
    // v4：kv_state 表。承载"单值状态"系列条目：current_mood / persona_summary
    // / daily_plan / 各种 daily_review_* 等。每条一个 key，value 是 raw 文本
    // （markdown / 自由文本），updated_at 自动维护。
    //
    // 用单表 + key 列而非每项一表：这些 entry 是 schema-less 文本，且数量
    // 是开放集（LLM 可能未来追加新单值），单表 + key 模式 future-proof。
    if current < 4 {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS kv_state (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            INSERT INTO _migrations (version, applied_at) VALUES (4, datetime('now'));",
        )?;
    }
    Ok(())
}

// ---- butler_tasks CRUD ----
//
// 本节函数接受 `&Connection` 参数（而非内部 with_db）—— 便于单测注入
// in-memory connection。Tauri command wrapper（v3 迭代时加）会用 with_db
// 包一层。

/// 列出全部 butler_tasks，按 updated_at desc。caller 想过滤 status 自己
/// `.iter().filter(|r| r.status == "pending")`；v1 不复杂化 SQL 接口。
pub fn butler_tasks_list(conn: &Connection) -> Result<Vec<ButlerTaskRow>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT title, description, status, detail_path, tags_json, created_at, updated_at
         FROM butler_tasks
         ORDER BY updated_at DESC",
    )?;
    let rows = stmt.query_map([], row_to_task)?;
    rows.collect()
}

/// 按 title 唯一索引查一条；不存在返 None。
pub fn butler_task_get(
    conn: &Connection,
    title: &str,
) -> Result<Option<ButlerTaskRow>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT title, description, status, detail_path, tags_json, created_at, updated_at
         FROM butler_tasks WHERE title = ?1",
    )?;
    stmt.query_row(params![title], row_to_task).optional()
}

/// Insert 一条。title 唯一冲突时返 Err；caller 想 upsert 自己先 get / update。
/// created_at / updated_at 留空 → 自动填 now。tags 用 JSON array 序列化进
/// tags_json 列。status 留空 → 默认 "pending"。
pub fn butler_task_create(
    conn: &Connection,
    row: &ButlerTaskRow,
) -> Result<(), rusqlite::Error> {
    let now = now_iso();
    let created_at = if row.created_at.is_empty() {
        now.clone()
    } else {
        row.created_at.clone()
    };
    let updated_at = if row.updated_at.is_empty() {
        now.clone()
    } else {
        row.updated_at.clone()
    };
    let status = if row.status.is_empty() {
        "pending".to_string()
    } else {
        row.status.clone()
    };
    let tags_json = serde_json::to_string(&row.tags).unwrap_or_else(|_| "[]".to_string());
    conn.execute(
        "INSERT INTO butler_tasks
            (title, description, status, detail_path, tags_json, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            row.title,
            row.description,
            status,
            row.detail_path,
            tags_json,
            created_at,
            updated_at,
        ],
    )?;
    Ok(())
}

/// Update by title。description / status / detail_path / tags 全字段覆盖；
/// caller 想做 partial update 自己先 get + 合并。updated_at 自动刷新。
/// 不存在 title → Ok(false)；存在并改了 → Ok(true)。
pub fn butler_task_update(
    conn: &Connection,
    title: &str,
    description: &str,
    status: &str,
    detail_path: Option<&str>,
    tags: &[String],
) -> Result<bool, rusqlite::Error> {
    let tags_json = serde_json::to_string(tags).unwrap_or_else(|_| "[]".to_string());
    let updated_at = now_iso();
    let n = conn.execute(
        "UPDATE butler_tasks
         SET description = ?2,
             status = ?3,
             detail_path = ?4,
             tags_json = ?5,
             updated_at = ?6
         WHERE title = ?1",
        params![title, description, status, detail_path, tags_json, updated_at],
    )?;
    Ok(n > 0)
}

/// Delete by title。不存在返 Ok(false)。
pub fn butler_task_delete(conn: &Connection, title: &str) -> Result<bool, rusqlite::Error> {
    let n = conn.execute("DELETE FROM butler_tasks WHERE title = ?1", params![title])?;
    Ok(n > 0)
}

// ---- todo CRUD（v7：与 butler_tasks 同字段集，schema 共享；status 默认
//      "active"，因为 todo 没有 done/error/cancelled 状态机。代码复用了
//      row_to_task helper —— 同样的列名提取。
//      仍各起独立 fn 而不抽象 trait —— 不同表名 caller 期望明确 fn 名，
//      额外抽象层换 token 复用收益不大。

pub type TodoRow = ButlerTaskRow; // schema 完全同形，类型别名复用。

pub fn todos_list(conn: &Connection) -> Result<Vec<TodoRow>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT title, description, status, detail_path, tags_json, created_at, updated_at
         FROM todo
         ORDER BY updated_at DESC",
    )?;
    let rows = stmt.query_map([], row_to_task)?;
    rows.collect()
}

pub fn todo_get(conn: &Connection, title: &str) -> Result<Option<TodoRow>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT title, description, status, detail_path, tags_json, created_at, updated_at
         FROM todo WHERE title = ?1",
    )?;
    stmt.query_row(params![title], row_to_task).optional()
}

pub fn todo_create(conn: &Connection, row: &TodoRow) -> Result<(), rusqlite::Error> {
    let now = now_iso();
    let created_at = if row.created_at.is_empty() {
        now.clone()
    } else {
        row.created_at.clone()
    };
    let updated_at = if row.updated_at.is_empty() {
        now
    } else {
        row.updated_at.clone()
    };
    let status = if row.status.is_empty() {
        "active".to_string()
    } else {
        row.status.clone()
    };
    let tags_json = serde_json::to_string(&row.tags).unwrap_or_else(|_| "[]".to_string());
    conn.execute(
        "INSERT INTO todo
            (title, description, status, detail_path, tags_json, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            row.title,
            row.description,
            status,
            row.detail_path,
            tags_json,
            created_at,
            updated_at,
        ],
    )?;
    Ok(())
}

pub fn todo_update(
    conn: &Connection,
    title: &str,
    description: &str,
    status: &str,
    detail_path: Option<&str>,
    tags: &[String],
) -> Result<bool, rusqlite::Error> {
    let tags_json = serde_json::to_string(tags).unwrap_or_else(|_| "[]".to_string());
    let updated_at = now_iso();
    let n = conn.execute(
        "UPDATE todo
         SET description = ?2,
             status = ?3,
             detail_path = ?4,
             tags_json = ?5,
             updated_at = ?6
         WHERE title = ?1",
        params![title, description, status, detail_path, tags_json, updated_at],
    )?;
    Ok(n > 0)
}

pub fn todo_delete(conn: &Connection, title: &str) -> Result<bool, rusqlite::Error> {
    let n = conn.execute("DELETE FROM todo WHERE title = ?1", params![title])?;
    Ok(n > 0)
}

/// 从 yaml 的 todo 段 backfill 到 SQLite。幂等。status 默认 "active"
/// （todo 没有 done/error/cancelled 标记需要派生）。
pub fn backfill_todos(
    conn: &Connection,
    items: &[crate::commands::memory::MemoryItem],
) -> Result<usize, String> {
    let mut inserted = 0usize;
    for item in items {
        let exists = todo_get(conn, &item.title)
            .map_err(|e| format!("get failed for {}: {e}", item.title))?
            .is_some();
        if exists {
            continue;
        }
        let detail_path = if item.detail_path.is_empty() {
            None
        } else {
            Some(item.detail_path.clone())
        };
        let row = TodoRow {
            title: item.title.clone(),
            description: item.description.clone(),
            status: "active".to_string(),
            detail_path,
            tags: crate::task_queue::parse_task_tags(&item.description),
            created_at: item.created_at.clone(),
            updated_at: item.updated_at.clone(),
        };
        todo_create(conn, &row)
            .map_err(|e| format!("create failed for {}: {e}", item.title))?;
        inserted += 1;
    }
    Ok(inserted)
}

pub fn startup_backfill_todos() {
    let idx = match crate::commands::memory::memory_list(Some("todo".to_string())) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("startup_backfill_todos: memory_list failed: {e}");
            return;
        }
    };
    let items: Vec<crate::commands::memory::MemoryItem> = idx
        .categories
        .get("todo")
        .map(|c| c.items.clone())
        .unwrap_or_default();
    let total = items.len();
    let result = with_db(|conn| match backfill_todos(conn, &items) {
        Ok(n) => Ok(n),
        Err(e) => {
            eprintln!("startup_backfill_todos: backfill failed: {e}");
            Ok(0usize)
        }
    });
    if let Ok(n) = result {
        if n > 0 {
            eprintln!("startup_backfill_todos: inserted {n} new todos (total yaml: {total})");
        }
    }
}

/// 把 todo 全表读出转 MemoryItem 形态，给 prompt builder / consolidate 等用。
pub fn todos_as_memory_items() -> Vec<crate::commands::memory::MemoryItem> {
    with_db(todos_list)
        .unwrap_or_default()
        .into_iter()
        .map(|r| r.to_memory_item())
        .collect()
}

// ---- todo 双写 mirror（v7：与 butler 同模式，best-effort eprintln on err）。

pub fn mirror_todo_create(item: &crate::commands::memory::MemoryItem) {
    let detail_path = if item.detail_path.is_empty() {
        None
    } else {
        Some(item.detail_path.clone())
    };
    let row = TodoRow {
        title: item.title.clone(),
        description: item.description.clone(),
        status: "active".to_string(),
        detail_path,
        tags: crate::task_queue::parse_task_tags(&item.description),
        created_at: item.created_at.clone(),
        updated_at: item.updated_at.clone(),
    };
    if let Err(e) = with_db(|conn| todo_create(conn, &row)) {
        eprintln!(
            "mirror_todo_create({}) failed (yaml succeeded, db skipped): {e}",
            item.title
        );
    }
}

pub fn mirror_todo_update(item: &crate::commands::memory::MemoryItem) {
    let tags = crate::task_queue::parse_task_tags(&item.description);
    let detail_path = if item.detail_path.is_empty() {
        None
    } else {
        Some(item.detail_path.as_str())
    };
    let title = item.title.clone();
    let desc = item.description.clone();
    if let Err(e) = with_db(|conn| {
        let n = todo_update(conn, &title, &desc, "active", detail_path, &tags)?;
        if !n {
            todo_create(
                conn,
                &TodoRow {
                    title: item.title.clone(),
                    description: item.description.clone(),
                    status: "active".to_string(),
                    detail_path: detail_path.map(|s| s.to_string()),
                    tags: tags.clone(),
                    created_at: item.created_at.clone(),
                    updated_at: item.updated_at.clone(),
                },
            )?;
        }
        Ok(())
    }) {
        eprintln!(
            "mirror_todo_update({}) failed (yaml succeeded, db skipped): {e}",
            item.title
        );
    }
}

pub fn mirror_todo_delete(title: &str) {
    if let Err(e) = with_db(|conn| todo_delete(conn, title)) {
        eprintln!("mirror_todo_delete({title}) failed (yaml succeeded, db skipped): {e}");
    }
}

pub fn mirror_todo_rename(old_title: &str, new_item: &crate::commands::memory::MemoryItem) {
    if let Err(e) = with_db(|conn| {
        let _ = todo_delete(conn, old_title)?;
        Ok(())
    }) {
        eprintln!("mirror_todo_rename({old_title}) delete-old failed: {e}");
    }
    mirror_todo_create(new_item);
}

// ---- task_archive CRUD（v8：schema 与 butler_tasks 同形，status 默认
//      "archived"。append-only 区，但仍提供 update / delete 以支持 LLM
//      memory_edit 任意操作（虽然实践中 archive 极少被改）。

pub type TaskArchiveRow = ButlerTaskRow;

pub fn task_archive_list(conn: &Connection) -> Result<Vec<TaskArchiveRow>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT title, description, status, detail_path, tags_json, created_at, updated_at
         FROM task_archive
         ORDER BY updated_at DESC",
    )?;
    let rows = stmt.query_map([], row_to_task)?;
    rows.collect()
}

pub fn task_archive_get(
    conn: &Connection,
    title: &str,
) -> Result<Option<TaskArchiveRow>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT title, description, status, detail_path, tags_json, created_at, updated_at
         FROM task_archive WHERE title = ?1",
    )?;
    stmt.query_row(params![title], row_to_task).optional()
}

pub fn task_archive_create(
    conn: &Connection,
    row: &TaskArchiveRow,
) -> Result<(), rusqlite::Error> {
    let now = now_iso();
    let created_at = if row.created_at.is_empty() {
        now.clone()
    } else {
        row.created_at.clone()
    };
    let updated_at = if row.updated_at.is_empty() {
        now
    } else {
        row.updated_at.clone()
    };
    let status = if row.status.is_empty() {
        "archived".to_string()
    } else {
        row.status.clone()
    };
    let tags_json = serde_json::to_string(&row.tags).unwrap_or_else(|_| "[]".to_string());
    conn.execute(
        "INSERT INTO task_archive
            (title, description, status, detail_path, tags_json, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            row.title,
            row.description,
            status,
            row.detail_path,
            tags_json,
            created_at,
            updated_at,
        ],
    )?;
    Ok(())
}

pub fn task_archive_update(
    conn: &Connection,
    title: &str,
    description: &str,
    status: &str,
    detail_path: Option<&str>,
    tags: &[String],
) -> Result<bool, rusqlite::Error> {
    let tags_json = serde_json::to_string(tags).unwrap_or_else(|_| "[]".to_string());
    let updated_at = now_iso();
    let n = conn.execute(
        "UPDATE task_archive
         SET description = ?2,
             status = ?3,
             detail_path = ?4,
             tags_json = ?5,
             updated_at = ?6
         WHERE title = ?1",
        params![title, description, status, detail_path, tags_json, updated_at],
    )?;
    Ok(n > 0)
}

pub fn task_archive_delete(conn: &Connection, title: &str) -> Result<bool, rusqlite::Error> {
    let n = conn.execute("DELETE FROM task_archive WHERE title = ?1", params![title])?;
    Ok(n > 0)
}

pub fn backfill_task_archive(
    conn: &Connection,
    items: &[crate::commands::memory::MemoryItem],
) -> Result<usize, String> {
    let mut inserted = 0usize;
    for item in items {
        let exists = task_archive_get(conn, &item.title)
            .map_err(|e| format!("get failed for {}: {e}", item.title))?
            .is_some();
        if exists {
            continue;
        }
        let detail_path = if item.detail_path.is_empty() {
            None
        } else {
            Some(item.detail_path.clone())
        };
        let row = TaskArchiveRow {
            title: item.title.clone(),
            description: item.description.clone(),
            status: "archived".to_string(),
            detail_path,
            tags: crate::task_queue::parse_task_tags(&item.description),
            created_at: item.created_at.clone(),
            updated_at: item.updated_at.clone(),
        };
        task_archive_create(conn, &row)
            .map_err(|e| format!("create failed for {}: {e}", item.title))?;
        inserted += 1;
    }
    Ok(inserted)
}

pub fn startup_backfill_task_archive() {
    let idx = match crate::commands::memory::memory_list(Some("task_archive".to_string())) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("startup_backfill_task_archive: memory_list failed: {e}");
            return;
        }
    };
    let items: Vec<crate::commands::memory::MemoryItem> = idx
        .categories
        .get("task_archive")
        .map(|c| c.items.clone())
        .unwrap_or_default();
    let total = items.len();
    let result = with_db(|conn| match backfill_task_archive(conn, &items) {
        Ok(n) => Ok(n),
        Err(e) => {
            eprintln!("startup_backfill_task_archive: backfill failed: {e}");
            Ok(0usize)
        }
    });
    if let Ok(n) = result {
        if n > 0 {
            eprintln!(
                "startup_backfill_task_archive: inserted {n} new (total yaml: {total})"
            );
        }
    }
}

pub fn task_archive_as_memory_items() -> Vec<crate::commands::memory::MemoryItem> {
    with_db(task_archive_list)
        .unwrap_or_default()
        .into_iter()
        .map(|r| r.to_memory_item())
        .collect()
}

pub fn mirror_archive_create(item: &crate::commands::memory::MemoryItem) {
    let detail_path = if item.detail_path.is_empty() {
        None
    } else {
        Some(item.detail_path.clone())
    };
    let row = TaskArchiveRow {
        title: item.title.clone(),
        description: item.description.clone(),
        status: "archived".to_string(),
        detail_path,
        tags: crate::task_queue::parse_task_tags(&item.description),
        created_at: item.created_at.clone(),
        updated_at: item.updated_at.clone(),
    };
    if let Err(e) = with_db(|conn| task_archive_create(conn, &row)) {
        eprintln!(
            "mirror_archive_create({}) failed (yaml succeeded, db skipped): {e}",
            item.title
        );
    }
}

pub fn mirror_archive_update(item: &crate::commands::memory::MemoryItem) {
    let tags = crate::task_queue::parse_task_tags(&item.description);
    let detail_path = if item.detail_path.is_empty() {
        None
    } else {
        Some(item.detail_path.as_str())
    };
    let title = item.title.clone();
    let desc = item.description.clone();
    if let Err(e) = with_db(|conn| {
        let n = task_archive_update(conn, &title, &desc, "archived", detail_path, &tags)?;
        if !n {
            task_archive_create(
                conn,
                &TaskArchiveRow {
                    title: item.title.clone(),
                    description: item.description.clone(),
                    status: "archived".to_string(),
                    detail_path: detail_path.map(|s| s.to_string()),
                    tags: tags.clone(),
                    created_at: item.created_at.clone(),
                    updated_at: item.updated_at.clone(),
                },
            )?;
        }
        Ok(())
    }) {
        eprintln!(
            "mirror_archive_update({}) failed (yaml succeeded, db skipped): {e}",
            item.title
        );
    }
}

pub fn mirror_archive_delete(title: &str) {
    if let Err(e) = with_db(|conn| task_archive_delete(conn, title)) {
        eprintln!("mirror_archive_delete({title}) failed (yaml succeeded, db skipped): {e}");
    }
}

pub fn mirror_archive_rename(
    old_title: &str,
    new_item: &crate::commands::memory::MemoryItem,
) {
    if let Err(e) = with_db(|conn| {
        let _ = task_archive_delete(conn, old_title)?;
        Ok(())
    }) {
        eprintln!("mirror_archive_rename({old_title}) delete-old failed: {e}");
    }
    mirror_archive_create(new_item);
}

// ---- kv_state (v9): 单值状态条目（mood / persona_summary / daily_plan / ...）
//      公共 key/value 抽象。

/// 读 kv_state 一条。不存在返 None；底层失败也返 None（caller 静默退化，
/// 这是 best-effort 状态读，不影响主流程）。
pub fn kv_get(key: &str) -> Option<String> {
    with_db(|conn| {
        let mut stmt = conn.prepare("SELECT value FROM kv_state WHERE key = ?1")?;
        stmt.query_row(params![key], |row| row.get::<_, String>(0))
            .optional()
    })
    .ok()
    .flatten()
}

/// 写 kv_state 一条（upsert）。失败 eprintln 不 panic —— 单值写失败属
/// best-effort 一致性问题，不影响主流程。
pub fn kv_set(key: &str, value: &str) {
    let now = now_iso();
    let res = with_db(|conn| {
        conn.execute(
            "INSERT INTO kv_state (key, value, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            params![key, value, now],
        )?;
        Ok(())
    });
    if let Err(e) = res {
        eprintln!("kv_set({key}) failed: {e}");
    }
}

/// 删 kv_state 一条。不存在视作成功（DELETE noop）。失败 eprintln。
pub fn kv_delete(key: &str) {
    let res = with_db(|conn| {
        conn.execute("DELETE FROM kv_state WHERE key = ?1", params![key])?;
        Ok(())
    });
    if let Err(e) = res {
        eprintln!("kv_delete({key}) failed: {e}");
    }
}

/// 取 kv_state 一条的 value + updated_at（caller 需要时间戳，如
/// get_persona_summary 给前端展示"X 天前更新"）。失败 / 不存在返 None。
pub fn kv_get_with_updated_at(key: &str) -> Option<(String, String)> {
    with_db(|conn| {
        let mut stmt = conn.prepare("SELECT value, updated_at FROM kv_state WHERE key = ?1")?;
        stmt.query_row(params![key], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .optional()
    })
    .ok()
    .flatten()
}

/// upsert 仅在 key 不存在时插入。给 backfill 用，避免覆盖已有 kv 写入。
pub fn kv_set_if_absent(key: &str, value: &str, updated_at: &str) {
    let res = with_db(|conn| {
        conn.execute(
            "INSERT INTO kv_state (key, value, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO NOTHING",
            params![key, value, updated_at],
        )?;
        Ok(())
    });
    if let Err(e) = res {
        eprintln!("kv_set_if_absent({key}) failed: {e}");
    }
}

// ---- ai_insights → kv_state mirror（v10）：persona_summary / daily_plan
//      / daily_review_<date> 等单值条目都同步到 kv_state。memory_edit 拦截
//      mirror_ai_insights_* 让 LLM 通过 memory_edit 写时透明双写。

pub fn mirror_ai_insights_create(item: &crate::commands::memory::MemoryItem) {
    crate::db::kv_set(&item.title, &item.description);
}

pub fn mirror_ai_insights_update(item: &crate::commands::memory::MemoryItem) {
    crate::db::kv_set(&item.title, &item.description);
}

pub fn mirror_ai_insights_delete(title: &str) {
    crate::db::kv_delete(title);
}

pub fn mirror_ai_insights_rename(old_title: &str, new_item: &crate::commands::memory::MemoryItem) {
    crate::db::kv_delete(old_title);
    crate::db::kv_set(&new_item.title, &new_item.description);
}

/// 启动时把 yaml ai_insights 段每条 MemoryItem 写到 kv_state（用
/// kv_set_if_absent 避免覆盖已存在的 kv 写）。failure 静默 —— mood 路径
/// 已 SQLite-first；ai_insights 只是单值 read 优化，缺也能 fallback yaml。
pub fn startup_backfill_ai_insights() {
    let idx = match crate::commands::memory::memory_list(Some("ai_insights".to_string())) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("startup_backfill_ai_insights: memory_list failed: {e}");
            return;
        }
    };
    let items: Vec<crate::commands::memory::MemoryItem> = idx
        .categories
        .get("ai_insights")
        .map(|c| c.items.clone())
        .unwrap_or_default();
    let mut inserted = 0;
    for item in &items {
        // 用 item.updated_at 保留原 yaml 时间戳；kv_set_if_absent 已存
        // 在的 key 不动 —— 让 memory_edit 后续 mirror 的 newer 写不会
        // 被 backfill 覆盖。
        let updated_at = if item.updated_at.is_empty() {
            chrono::Local::now()
                .format("%Y-%m-%dT%H:%M:%S%:z")
                .to_string()
        } else {
            item.updated_at.clone()
        };
        // 检查是否已存在：不存在才 insert + 计数
        if kv_get(&item.title).is_none() {
            kv_set_if_absent(&item.title, &item.description, &updated_at);
            inserted += 1;
        }
    }
    if inserted > 0 {
        eprintln!(
            "startup_backfill_ai_insights: inserted {inserted} kv entries (total yaml: {})",
            items.len()
        );
    }
}

/// 一次性把 butler_tasks 全表读出来转成 MemoryItem 形态，给 prompt builder /
/// task_heartbeat 等"想拿一组 task" 的 caller 用。失败 → 空 Vec（caller 静默
/// 退化，与之前 yaml memory_list 失败时 `unwrap_or_default` 同语义）。
pub fn butler_tasks_as_memory_items() -> Vec<crate::commands::memory::MemoryItem> {
    with_db(butler_tasks_list)
        .unwrap_or_default()
        .into_iter()
        .map(|r| r.to_memory_item())
        .collect()
}

// ---- 双写 helpers（v4）：让 memory_edit 在 butler_tasks 上同步写 SQLite。
// 这些函数是 best-effort —— 失败 eprintln 不 panic，让 yaml 主路径仍走。

/// 镜像 memory_edit("create", "butler_tasks", ...) 写 SQLite。从派生 status /
/// tags 与 v2 backfill 同算法。
pub fn mirror_butler_create(item: &crate::commands::memory::MemoryItem) {
    let (status_enum, _) = crate::task_queue::classify_status(&item.description);
    let status = match status_enum {
        crate::task_queue::TaskStatus::Pending => "pending",
        crate::task_queue::TaskStatus::Done => "done",
        crate::task_queue::TaskStatus::Error => "error",
        crate::task_queue::TaskStatus::Cancelled => "cancelled",
    }
    .to_string();
    let tags = crate::task_queue::parse_task_tags(&item.description);
    let detail_path = if item.detail_path.is_empty() {
        None
    } else {
        Some(item.detail_path.clone())
    };
    let row = ButlerTaskRow {
        title: item.title.clone(),
        description: item.description.clone(),
        status,
        detail_path,
        tags,
        created_at: item.created_at.clone(),
        updated_at: item.updated_at.clone(),
    };
    if let Err(e) = with_db(|conn| butler_task_create(conn, &row)) {
        eprintln!(
            "mirror_butler_create({}) failed (yaml succeeded, db skipped): {e}",
            item.title
        );
    }
}

/// 镜像 memory_edit("update", "butler_tasks", ...) 写 SQLite。
pub fn mirror_butler_update(item: &crate::commands::memory::MemoryItem) {
    let (status_enum, _) = crate::task_queue::classify_status(&item.description);
    let status = match status_enum {
        crate::task_queue::TaskStatus::Pending => "pending",
        crate::task_queue::TaskStatus::Done => "done",
        crate::task_queue::TaskStatus::Error => "error",
        crate::task_queue::TaskStatus::Cancelled => "cancelled",
    };
    let tags = crate::task_queue::parse_task_tags(&item.description);
    let detail_path = if item.detail_path.is_empty() {
        None
    } else {
        Some(item.detail_path.as_str())
    };
    let title = item.title.clone();
    let desc = item.description.clone();
    if let Err(e) = with_db(|conn| {
        // 如果 update 命中 0 行（title 不在 SQLite —— 例如 yaml 早就有但
        // 启动 backfill 时漏掉），fallback 到 create 让两边对齐。
        let n = butler_task_update(conn, &title, &desc, status, detail_path, &tags)?;
        if !n {
            butler_task_create(
                conn,
                &ButlerTaskRow {
                    title: item.title.clone(),
                    description: item.description.clone(),
                    status: status.to_string(),
                    detail_path: detail_path.map(|s| s.to_string()),
                    tags: tags.clone(),
                    created_at: item.created_at.clone(),
                    updated_at: item.updated_at.clone(),
                },
            )?;
        }
        Ok(())
    }) {
        eprintln!(
            "mirror_butler_update({}) failed (yaml succeeded, db skipped): {e}",
            item.title
        );
    }
}

/// 镜像 memory_edit("delete", "butler_tasks", ...) 写 SQLite。
pub fn mirror_butler_delete(title: &str) {
    if let Err(e) = with_db(|conn| butler_task_delete(conn, title)) {
        eprintln!(
            "mirror_butler_delete({title}) failed (yaml succeeded, db skipped): {e}"
        );
    }
}

/// 镜像 memory_rename：SQLite 的 UNIQUE title 索引让我们 ALTER 不了 title，
/// 改成 delete 旧 + insert 新（保留其它字段）。
pub fn mirror_butler_rename(
    old_title: &str,
    new_item: &crate::commands::memory::MemoryItem,
) {
    if let Err(e) = with_db(|conn| {
        // 取旧 row 保留 created_at；新 title 走完整 ButlerTaskRow create
        let _ = butler_task_delete(conn, old_title)?;
        Ok(())
    }) {
        eprintln!(
            "mirror_butler_rename({old_title}) delete-old failed: {e}"
        );
    }
    mirror_butler_create(new_item);
}

// ---- Tauri commands ----
//
// v3：仅暴露只读 list 让前端可独立验证。写路径仍走 memory_edit，v4 才双写。

/// 列出 SQLite 里的 butler_tasks。前端可用此与 memory_list 对比，验证
/// backfill 是否成功 + db schema 是否预期。
#[tauri::command]
pub fn db_butler_tasks_list() -> Result<Vec<ButlerTaskRow>, String> {
    with_db(butler_tasks_list)
}

/// 启动时从 yaml 回填 butler_tasks 到 SQLite。幂等（已存在 title 跳过），
/// 多次启动只插一次。**同步**执行 —— v5 切读路径后，read 不能在 backfill
/// 完之前 race；几百条 task 的 INSERT < 10ms，阻塞启动可接受。失败 eprintln
/// 不 panic（让 app 继续跑，下次启动重试）。
pub fn startup_backfill_butler_tasks() {
    let idx = match crate::commands::memory::memory_list(Some("butler_tasks".to_string())) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("startup_backfill_butler_tasks: memory_list failed: {e}");
            return;
        }
    };
    let items: Vec<crate::commands::memory::MemoryItem> = idx
        .categories
        .get("butler_tasks")
        .map(|c| c.items.clone())
        .unwrap_or_default();
    let total = items.len();
    let result = with_db(|conn| match backfill_butler_tasks(conn, &items) {
        Ok(n) => Ok(n),
        Err(e) => {
            eprintln!("startup_backfill_butler_tasks: backfill failed: {e}");
            Ok(0usize)
        }
    });
    match result {
        Ok(n) if n > 0 => {
            eprintln!(
                "startup_backfill_butler_tasks: inserted {n} new tasks (total yaml: {total})"
            );
        }
        Ok(_) => {
            // 0 插入是正常路径（既有 db 已 backfill 过 / yaml 空）
        }
        Err(e) => {
            eprintln!("startup_backfill_butler_tasks: with_db failed: {e}");
        }
    }
}

/// 从 memory_index.yaml 的 butler_tasks 段把每条 MemoryItem 派生成
/// ButlerTaskRow 插入 SQLite。已存在的 title 跳过（幂等），返新插入数。
///
/// status / tags 从 description 文本派生：
/// - status 用 `task_queue::classify_status` 同算法
/// - tags 用 `task_queue::parse_task_tags` 同算法
///
/// detail_path: 空串 → None；非空 → Some。
///
/// 设计为接 `Vec<&MemoryItem>` 而非内部 `memory_list()`，让单测能注入
/// 任意 items 集合不依赖磁盘 yaml。caller 在 v3 / v4 把 memory_list
/// 输出传进来即可。
pub fn backfill_butler_tasks(
    conn: &Connection,
    items: &[crate::commands::memory::MemoryItem],
) -> Result<usize, String> {
    let mut inserted = 0usize;
    for item in items {
        // 已存在的 title 跳过（让 backfill 幂等，多次启动只插一次）
        let exists = butler_task_get(conn, &item.title)
            .map_err(|e| format!("get failed for {}: {e}", item.title))?
            .is_some();
        if exists {
            continue;
        }
        let (status_enum, _) = crate::task_queue::classify_status(&item.description);
        let status = match status_enum {
            crate::task_queue::TaskStatus::Pending => "pending",
            crate::task_queue::TaskStatus::Done => "done",
            crate::task_queue::TaskStatus::Error => "error",
            crate::task_queue::TaskStatus::Cancelled => "cancelled",
        }
        .to_string();
        let tags = crate::task_queue::parse_task_tags(&item.description);
        let detail_path = if item.detail_path.is_empty() {
            None
        } else {
            Some(item.detail_path.clone())
        };
        let row = ButlerTaskRow {
            title: item.title.clone(),
            description: item.description.clone(),
            status,
            detail_path,
            tags,
            created_at: item.created_at.clone(),
            updated_at: item.updated_at.clone(),
        };
        butler_task_create(conn, &row)
            .map_err(|e| format!("create failed for {}: {e}", item.title))?;
        inserted += 1;
    }
    Ok(inserted)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 启动 + migration 必须可重入。第一次建表，第二次 noop。
    #[test]
    fn migrations_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        apply_migrations(&conn).unwrap();
        apply_migrations(&conn).unwrap();
        // 验证 butler_tasks 表存在
        let count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='butler_tasks'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "butler_tasks table should exist after migration");
        // todo table 也应建好（v2 migration）
        let todo_count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='todo'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(todo_count, 1, "todo table should exist after migration");
        let archive_count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='task_archive'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(archive_count, 1, "task_archive table should exist after migration");
        let kv_count: i32 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='kv_state'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(kv_count, 1, "kv_state table should exist after migration");
        // _migrations table 每个版本一行（v1 + v2 + v3 + v4 = 4；noop 第二次不重复）
        let mig_count: i32 = conn
            .query_row("SELECT COUNT(*) FROM _migrations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(mig_count, 4, "four migrations applied, noop second call");
    }

    #[test]
    fn kv_state_upsert_and_delete() {
        // kv_get / kv_set / kv_delete 用全局 `with_db`，所以这里不能用
        // fresh_conn —— 直接走全局连接做 round-trip。pet.db 是用户配置
        // 目录下的实际文件；CI / 单机测试都能写。end 时清掉避免污染。
        let key = "_test_kv_state_upsert";
        crate::db::kv_set(key, "first");
        assert_eq!(crate::db::kv_get(key).as_deref(), Some("first"));
        crate::db::kv_set(key, "second");
        assert_eq!(crate::db::kv_get(key).as_deref(), Some("second"));
        crate::db::kv_delete(key);
        assert_eq!(crate::db::kv_get(key), None);
    }

    #[test]
    fn task_archive_crud_roundtrip() {
        let conn = fresh_conn();
        let row = TaskArchiveRow {
            title: "2026-04-01_整理 Downloads".to_string(),
            description: "[archived: 2026-04-01] [task pri=3] 整理 [done]".to_string(),
            status: "archived".to_string(),
            detail_path: Some("task_archive/2026_04_01_zheng_li.md".to_string()),
            tags: vec![],
            created_at: "".to_string(),
            updated_at: "".to_string(),
        };
        task_archive_create(&conn, &row).unwrap();
        let fetched = task_archive_get(&conn, "2026-04-01_整理 Downloads")
            .unwrap()
            .unwrap();
        assert_eq!(fetched.status, "archived");
        let deleted = task_archive_delete(&conn, "2026-04-01_整理 Downloads").unwrap();
        assert!(deleted);
    }

    fn fresh_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        apply_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn crud_roundtrip() {
        let conn = fresh_conn();
        let row = ButlerTaskRow {
            title: "整理 Downloads".to_string(),
            description: "[once: 2026-05-12] 把过期截图归类".to_string(),
            status: "pending".to_string(),
            detail_path: Some("butler_tasks/zheng_li_downloads.md".to_string()),
            tags: vec!["生活".to_string(), "整理".to_string()],
            created_at: "".to_string(),
            updated_at: "".to_string(),
        };
        butler_task_create(&conn, &row).unwrap();
        let fetched = butler_task_get(&conn, "整理 Downloads").unwrap().unwrap();
        assert_eq!(fetched.title, row.title);
        assert_eq!(fetched.description, row.description);
        assert_eq!(fetched.status, "pending");
        assert_eq!(fetched.tags, vec!["生活", "整理"]);
        assert!(!fetched.created_at.is_empty(), "created_at auto-filled");
        assert!(!fetched.updated_at.is_empty(), "updated_at auto-filled");

        let updated = butler_task_update(
            &conn,
            "整理 Downloads",
            "[once: 2026-05-12] [done] 完成",
            "done",
            None,
            &["生活".to_string()],
        )
        .unwrap();
        assert!(updated);
        let after = butler_task_get(&conn, "整理 Downloads").unwrap().unwrap();
        assert_eq!(after.status, "done");
        assert_eq!(after.detail_path, None);
        assert_eq!(after.tags, vec!["生活"]);

        let deleted = butler_task_delete(&conn, "整理 Downloads").unwrap();
        assert!(deleted);
        assert!(butler_task_get(&conn, "整理 Downloads").unwrap().is_none());
    }

    #[test]
    fn create_unique_title() {
        let conn = fresh_conn();
        let row = ButlerTaskRow {
            title: "唯一".to_string(),
            description: "first".to_string(),
            status: "pending".to_string(),
            detail_path: None,
            tags: vec![],
            created_at: "".to_string(),
            updated_at: "".to_string(),
        };
        butler_task_create(&conn, &row).unwrap();
        let err = butler_task_create(&conn, &row);
        assert!(err.is_err(), "duplicate title should be rejected by UNIQUE");
    }

    #[test]
    fn update_missing_title_returns_false() {
        let conn = fresh_conn();
        let updated =
            butler_task_update(&conn, "不存在", "desc", "pending", None, &[]).unwrap();
        assert!(!updated);
    }

    #[test]
    fn backfill_derives_status_and_tags() {
        use crate::commands::memory::MemoryItem;
        let conn = fresh_conn();
        let items = vec![
            MemoryItem {
                title: "做完的事".to_string(),
                description: "[task pri=3] 整理 #生活 [done] [result: 38 文件归档]".to_string(),
                detail_path: "butler_tasks/zuo_wan_de_shi.md".to_string(),
                created_at: "2026-05-01T00:00:00+08:00".to_string(),
                updated_at: "2026-05-02T00:00:00+08:00".to_string(),
            },
            MemoryItem {
                title: "失败的事".to_string(),
                description: "[task pri=5] 写日报 #工作 [error: 网络超时]".to_string(),
                detail_path: "".to_string(),
                created_at: "2026-05-01T00:00:00+08:00".to_string(),
                updated_at: "2026-05-03T00:00:00+08:00".to_string(),
            },
            MemoryItem {
                title: "待办的事".to_string(),
                description: "[task pri=2] 倒垃圾 #家务".to_string(),
                detail_path: "butler_tasks/dao_la_ji.md".to_string(),
                created_at: "2026-05-04T00:00:00+08:00".to_string(),
                updated_at: "2026-05-04T00:00:00+08:00".to_string(),
            },
        ];
        let n = backfill_butler_tasks(&conn, &items).unwrap();
        assert_eq!(n, 3);

        let done = butler_task_get(&conn, "做完的事").unwrap().unwrap();
        assert_eq!(done.status, "done");
        assert_eq!(done.tags, vec!["生活"]);
        assert_eq!(done.detail_path, Some("butler_tasks/zuo_wan_de_shi.md".to_string()));

        let err = butler_task_get(&conn, "失败的事").unwrap().unwrap();
        assert_eq!(err.status, "error");
        assert_eq!(err.tags, vec!["工作"]);
        assert_eq!(err.detail_path, None, "empty detail_path → None");

        let pending = butler_task_get(&conn, "待办的事").unwrap().unwrap();
        assert_eq!(pending.status, "pending");
        assert_eq!(pending.tags, vec!["家务"]);

        // 再跑一次 backfill：幂等，不重复插
        let n2 = backfill_butler_tasks(&conn, &items).unwrap();
        assert_eq!(n2, 0, "re-backfill must skip existing titles");
    }

    #[test]
    fn todo_crud_roundtrip() {
        let conn = fresh_conn();
        let row = TodoRow {
            title: "周三 14:00 视频会议".to_string(),
            description: "[remind: 2026-05-14 14:00] 客户 demo".to_string(),
            status: "active".to_string(),
            detail_path: None,
            tags: vec![],
            created_at: "".to_string(),
            updated_at: "".to_string(),
        };
        todo_create(&conn, &row).unwrap();
        let fetched = todo_get(&conn, "周三 14:00 视频会议").unwrap().unwrap();
        assert_eq!(fetched.status, "active");
        assert!(!fetched.created_at.is_empty());

        let updated = todo_update(
            &conn,
            "周三 14:00 视频会议",
            "[remind: 2026-05-14 14:30] 客户 demo（推迟 30 分钟）",
            "active",
            None,
            &[],
        )
        .unwrap();
        assert!(updated);
        let after = todo_get(&conn, "周三 14:00 视频会议").unwrap().unwrap();
        assert!(after.description.contains("14:30"));

        let deleted = todo_delete(&conn, "周三 14:00 视频会议").unwrap();
        assert!(deleted);
        assert!(todo_get(&conn, "周三 14:00 视频会议").unwrap().is_none());
    }

    #[test]
    fn todo_backfill_skips_existing() {
        use crate::commands::memory::MemoryItem;
        let conn = fresh_conn();
        let items = vec![MemoryItem {
            title: "买菜".to_string(),
            description: "晚上买番茄鸡蛋".to_string(),
            detail_path: "".to_string(),
            created_at: "2026-05-01T00:00:00+08:00".to_string(),
            updated_at: "2026-05-01T00:00:00+08:00".to_string(),
        }];
        let n = backfill_todos(&conn, &items).unwrap();
        assert_eq!(n, 1);
        let n2 = backfill_todos(&conn, &items).unwrap();
        assert_eq!(n2, 0, "re-backfill must skip existing titles");
    }

    #[test]
    fn list_order_by_updated_at_desc() {
        let conn = fresh_conn();
        butler_task_create(
            &conn,
            &ButlerTaskRow {
                title: "A".to_string(),
                description: "".to_string(),
                status: "pending".to_string(),
                detail_path: None,
                tags: vec![],
                created_at: "2026-05-01T00:00:00+08:00".to_string(),
                updated_at: "2026-05-01T00:00:00+08:00".to_string(),
            },
        )
        .unwrap();
        butler_task_create(
            &conn,
            &ButlerTaskRow {
                title: "B".to_string(),
                description: "".to_string(),
                status: "pending".to_string(),
                detail_path: None,
                tags: vec![],
                created_at: "2026-05-02T00:00:00+08:00".to_string(),
                updated_at: "2026-05-02T00:00:00+08:00".to_string(),
            },
        )
        .unwrap();
        let rows = butler_tasks_list(&conn).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].title, "B", "newest updated_at first");
        assert_eq!(rows[1].title, "A");
    }
}
