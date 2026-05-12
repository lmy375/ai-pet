# SQLite v9：kv_state 表 + mood 迁移

## 背景

接 v8（task_archive 已迁）。GOAL 列出的 mood 是单值状态，已在 `~/.config/pet/current_mood.txt` 单文件。本轮把它迁到 SQLite，顺带建一个**通用** `kv_state` 表 hosting 所有"单值状态"系列（current_mood / persona_summary / daily_plan / daily_review_* 等），为 v10 迁 persona/plan 铺路。

## 改动

### 1. Schema（`db.rs::apply_migrations` v4）

```sql
CREATE TABLE kv_state (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
```

单表 + key 列而非每项一表 —— 单值条目数量是开放集（LLM 可能未来追加），future-proof。

### 2. KV helpers（`db.rs`）

- `kv_get(key) -> Option<String>` —— 不存在 / 失败返 None（best-effort 读，不影响主流程）
- `kv_set(key, value)` —— upsert（ON CONFLICT DO UPDATE），自动 updated_at；失败 eprintln
- `kv_delete(key)` —— DELETE，noop 视作成功

### 3. mood.rs 迁移

`record_current_mood / clear_current_mood / read_mood_file` 改成 **SQLite 优先 + 文件 fallback**：
- `record_current_mood(raw)`：先 `kv_set("current_mood", raw)`；再写 current_mood.txt（保留为回滚保险 + 外部读保险）
- `clear_current_mood()`：`kv_delete("current_mood")` + 删文件
- `read_mood_file()`：先 `kv_get("current_mood")`；空时 fallback 旧文件；fallback 命中后**一次性 backfill** 回 SQLite

### 4. 单测

- `kv_state_upsert_and_delete` —— set 两次（验 upsert）→ delete → get None
- 修正 `migrations_idempotent`（mig_count 3 → 4 + 验 kv_state 表存在）

## 不做

- 不迁 mood_history（append-only 时间序列，不是单值状态；shape 不属 kv_state）—— 单独评估
- 不删 current_mood.txt 文件 —— 保留双写
- 不动 mood 的 LLM 拦截层（memory_edit 拦截 ai_insights/current_mood 仍走 record_current_mood，自动走 SQLite）

## 验收

- `cargo build --release` ✅
- `cargo test --lib` ✅ 全 **880 通过**
- 启动后：
  - SQLite `kv_state` 表创建
  - 用户 LLM 写心情 → SQLite kv 立即生效 + 文件同步
  - 读心情先看 SQLite（升级用户首次启动会 backfill 旧文件值进 kv）

## SQLite 流水线

- v0–v6 ✅ butler_tasks
- v7 ✅ todo
- v8 ✅ task_archive
- v9 ✅ **kv_state + mood**
- v10 ⏳ 用 kv_state 迁 persona_summary / daily_plan / daily_review_*

## 完成

- [x] kv_state schema + helpers
- [x] mood 双写 + 读 SQLite-first 含 backfill
- [x] kv 单测 + 修正 migration 测试
- [x] 移到 docs/done/
