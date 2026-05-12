# SQLite v10：ai_insights → kv_state

## 背景

接 v9（kv_state 表 + mood 已迁）。本轮把 ai_insights category 的"计划进度"系列条目迁到 kv_state：
- `persona_summary` —— 宠物自我画像
- `daily_plan` —— 当日计划
- `daily_review_<date>` —— 每日复盘

它们都是单值文本（title + description），符合 kv_state 形态。

## 改动

### 1. `db.rs` 加 helpers

- `kv_get_with_updated_at(key) -> Option<(value, updated_at)>` —— caller 需要时间戳时用（如 `get_persona_summary` 给前端展示"X 天前更新"）
- `kv_set_if_absent(key, value, updated_at)` —— 仅当 key 不存在时插入。backfill 用，避免覆盖 mirror 新写

### 2. `db.rs` 加 4 个 mirror fn

- `mirror_ai_insights_create / update / delete / rename`
- 内部就是 `kv_set / kv_delete`，包装 fn 维持 mirror_* 命名一致性

### 3. `db.rs::startup_backfill_ai_insights`

- 读 yaml `ai_insights` 段 → 每条 `kv_set_if_absent` 写入
- 已有 kv 写不动（让 memory_edit mirror 的新写优先于 yaml 旧值）

### 4. `read_ai_insights_item`（commands/memory.rs）

旧：直接 `memory_list("ai_insights")` + iter find。
新：**kv 优先 fallback yaml**：
```rust
if let Some((value, updated_at)) = kv_get_with_updated_at(title) {
    return Some(MemoryItem { title, description: value, ... });
}
// fallback yaml ...
```

5 个 caller 受益（persona_summary 读 ×2 / daily_plan 读 ×2 / daily_review_<date> 存在检查 + 读）。

### 5. memory_edit / memory_rename 双写扩展

第 5 个 category 加入 mirror match：
```rust
match category.as_str() {
    "butler_tasks" => ..butler..,
    "todo" => ..todo..,
    "task_archive" => ..archive..,
    "ai_insights" => mirror_ai_insights_*(...),
    _ => {}
}
```

### 6. lib.rs setup hook

加 `db::startup_backfill_ai_insights()` 调用。

## 不做

- 不迁 `user_profile` / `general` —— 这两类是真正的"记忆" 类（GOAL 明确"memory 只承担大模型记忆/回想职责"），留 yaml。
- 不动 `memory_list` / `memory_search` 对 ai_insights 段的覆盖 —— ai_insights 仍以 yaml 段为主显（前端 PanelMemory 看到的 ai_insights 段 / LLM 用 memory_list 看到的是 yaml；只是单值 hot read（`read_ai_insights_item`）走 SQLite）。这保证 LLM 写习惯不变 + memory tab 继续展示这些"思考产物"。
- 不写新单测 —— `kv_state_upsert_and_delete` 已覆盖 kv 路径；ai_insights mirror 只是 kv 包装，靠 v9 测试和现有 read_ai_insights_item 路径手动验证。

## 验收

- `cargo build --release` ✅
- `cargo test --lib` ✅ 全 **880 通过**
- 启动后：
  - `kv_state` 里有 persona_summary / daily_plan / daily_review_<recent> 等行
  - 调 `get_persona_summary` / `read_daily_plan_description` / `read_daily_review_description` 都直接走 SQLite 单 SELECT，比 yaml 全扫快
  - LLM memory_edit ai_insights/persona_summary 时仍写 yaml + 同步 mirror 到 kv

## SQLite 流水线

- v0–v6 ✅ butler_tasks
- v7 ✅ todo
- v8 ✅ task_archive
- v9 ✅ kv_state + mood
- v10 ✅ **ai_insights → kv**（persona / plan / daily_review）

至此 GOAL 列出的所有 "搬出 memory" 域全部 SQLite 化：
- butler_tasks ✅
- todo ✅
- task_archive ✅
- mood ✅（kv_state）
- 计划进度（persona_summary / daily_plan / daily_review_*）✅（kv_state）

## 完成

- [x] kv_get_with_updated_at / kv_set_if_absent
- [x] 4 ai_insights mirror fn + startup_backfill
- [x] read_ai_insights_item kv-first
- [x] 5 memory_edit/rename 分支扩展
- [x] lib.rs hook
- [x] 移到 docs/done/
