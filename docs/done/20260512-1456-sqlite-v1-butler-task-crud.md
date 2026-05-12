# SQLite v1：butler_task CRUD 函数

## 背景

接 SQLite v0 foundation。schema 已建好，本轮加 list / get / create / update / delete 5 个 pure Rust 函数。函数接受 `&Connection`（不内部 with_db），便于单测注入 in-memory connection；Tauri command wrapper 留给 v3。

## 改动

### `src-tauri/src/db.rs` 扩展

#### 数据结构

新增 `ButlerTaskRow` struct：与 `MemoryItem` 字段对齐 + 显式 `status` + 显式 `tags: Vec<String>`：

```rust
pub struct ButlerTaskRow {
    pub title: String,
    pub description: String,
    pub status: String,
    pub detail_path: Option<String>,
    pub tags: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}
```

tags 走 JSON 序列化进 `tags_json` 列（SQLite 无 native array）。

#### 5 个 CRUD 函数

- `butler_tasks_list(conn)` —— 全表 ORDER BY updated_at DESC
- `butler_task_get(conn, title)` —— UNIQUE title 查询，返 Option
- `butler_task_create(conn, row)` —— Insert；created_at/updated_at 留空时自动 now()；status 留空默认 "pending"；title 冲突返 Err
- `butler_task_update(conn, title, desc, status, detail_path, tags)` —— 全字段覆盖，updated_at 自动 now()；title 不存在返 Ok(false)
- `butler_task_delete(conn, title)` —— 删除；不存在返 Ok(false)

#### 4 个单元测试

1. `crud_roundtrip` —— create → get → update → delete 全链路
2. `create_unique_title` —— UNIQUE 索引拒绝重复 title
3. `update_missing_title_returns_false` —— soft fail，不抛错
4. `list_order_by_updated_at_desc` —— 排序验证

全 5 个测试（含 v0 的 migrations_idempotent）通过：
```
test result: ok. 5 passed; 0 failed
```

## 不做

- 不挂 Tauri command（v3 才做）
- 不做 partial update（caller 自己先 get + merge）
- 不暴露 `status` 过滤 SQL —— v1 简单 list 全表，caller 内存过滤；后续真有性能需要再加 SQL where
- 不写 backfill（v2 做）

## 验收

- `cargo test --lib db::` 全 5 通过
- `cargo build --release` 通过
- 无任何 caller 接入，零功能变化

## 完成

- [x] ButlerTaskRow + 5 CRUD 函数
- [x] 4 个单元测试
- [x] 移到 docs/done/
