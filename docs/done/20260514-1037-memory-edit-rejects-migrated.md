# 持久化分层最后一公里：memory_edit LLM 面向硬拒已迁出 category

## 背景

`docs/TODO.md` 用户确认的剩余需求：

> 持久化分层重构（SQLite）：memory 只承担"大模型记忆/回想"职责；butler_tasks / todo / task_archive / 计划进度 / mood 等业务态搬出 memory，另建 sqlite 表（复用 memory 字段：title / description / created_at / updated_at / detail_path / tags / status）。LLM 通过专用工具读写各域，不再共用 memory_edit。

调查后发现 v0–v12 已经把绝大部分搬完：

- ✅ SQLite 层 `src-tauri/src/db.rs` 1813 行：`butler_tasks` / `todo` / `task_archive` / `kv_state` 表 + migration + mirror 双写 + startup backfill
- ✅ 读路径：`memory_list` / `search_memory` 对这几个 category 已经走 SQLite-as-truth（`butler_tasks_as_memory_items` 等）
- ✅ 专用 LLM 工具：`butler_task_edit` / `todo_edit` 已上线
- ✅ kv_state 装 mood / persona_summary / daily_plan / daily_review_*
- ✅ 各 caller（proactive / consolidate / chat / task_heartbeat）已经从 yaml 转向 SQLite 读

**缺口**：LLM 面向的 `memory_edit` 工具 schema enum 还接受 `butler_tasks` / `todo`，描述里也写"currently still works as a legacy fallback"。架构目标"LLM 通过专用工具读写各域，不再共用 memory_edit"卡在这"软引导"上 —— 实际占比 chip 仍可能看到 LLM 回退共用 surface 的 fallback 调用。

## 改动

### `src-tauri/src/tools/memory_tools.rs`

**Schema 收紧**：

- `category` enum 从 `["ai_insights", "user_profile", "todo", "butler_tasks", "general"]` 收敛为 `["ai_insights", "user_profile", "general"]`，让 LLM SDK 的 JSON schema validator 在调用前就挡掉 migrated category。
- description 改写：去掉"currently still works as a legacy fallback"措辞，明确写"memory_edit will REFUSE these categories"，并列 task_archive（read-only，由 consolidate 管理）。

**运行时硬拒**（双保险，schema 万一漏掉）：

```rust
if let Some(redirect) = dedicated_redirect_for(&category) {
    let err = serde_json::json!({
        "error": format!("memory_edit refuses category '{category}'. Use the dedicated tool: {redirect}."),
        "use_tool": redirect,
    });
    return err.to_string();
}
```

新 pure helper `dedicated_redirect_for`：

| category       | redirect                                            |
| -------------- | --------------------------------------------------- |
| `butler_tasks` | `butler_task_edit`                                  |
| `todo`         | `todo_edit`                                         |
| `task_archive` | `(read-only — managed by the consolidate loop)`     |
| 其它           | `None`（走原 memory_edit）                          |

**清理副作用**：原 `memory_edit_impl` 里 butler_history 事件记录路径（`butler_action_logged && category == "butler_tasks"`）随之删除 —— LLM 走不到 butler_tasks 这条 category 了。butler_task_edit 自己仍记录事件（既有路径不动）。

### `src-tauri/src/tool_call_history.rs`

更新 `DedicatedToolStats` 头部注释：稳态期望 `memory_edit_butler_count` / `memory_edit_todo_count` 为 0；保留计数作为"拒绝策略意外失效"的兜底告警。前端「🛠 专用工具占比」chip 渲染逻辑不变。

### `README.md`

第 8 节"持久化分层"更新：删"仍接受 fallback"措辞；点明"LLM 面向硬拒 + Schema enum 移除 + 运行时拦截 + 前端 invoke 路径不受影响"。

### 测试

`tools::memory_tools::tests::dedicated_redirect_table`（新增）：

- migrated 域必须返回 redirect（`butler_tasks` → `butler_task_edit`，`todo` → `todo_edit`，`task_archive` → Some(...)）
- memory-native 域必须返回 `None`（`ai_insights` / `user_profile` / `general`）
- 未知 category 返 `None`（让 `memory::memory_edit` 自己的"Unknown category"错误生效，不双重报错）

## 不做

- **不动 Tauri 命令 `memory_edit`**。前端 PanelMemory / PanelTasks `invoke("memory_edit", { category: "butler_tasks", ... })` 仍是合法路径 —— 那是面板编辑的真实 surface，与 LLM 工具是两条独立通道。Rust 层无法区分 caller 是 LLM 还是前端；本拒绝逻辑只在 `MemoryEditTool::execute` 这层 LLM-facing wrapper 里生效。
- **不动 `butler_task_edit_impl` / `todo_edit_impl` 内部路径**。它们仍调 `memory::memory_edit` 函数，走 yaml + SQLite mirror。如未来要把 yaml 完全砍掉，会是独立一波重构（涉及 `memory_index.yaml` 是否还保留 butler_tasks / todo 段、detail.md 文件生命周期等）。
- **不动 `ai_insights` 域**。`persona_summary` / `daily_plan` / `daily_review_*` 仍走 `memory_edit("ai_insights", ...)` + kv_state mirror；它们语义上属于"宠物记忆/回想"，留在 memory 域合理。
- **不动 `task_archive` 写路径**。LLM 本来就不该写 archive；consolidate 循环把 done/cancelled > 30 天的 butler_tasks 自动挪过去。新增的 `dedicated_redirect_for("task_archive")` 是为了 LLM 偶然路径上给清晰提示。

## 验证

- `cargo check` ✓ 编译通过（无新 warning，仅既有 `unused_imports` / `dead_code` 等无关项）
- `cargo test --lib` ✓ **921 / 921 通过**，含新增 `dedicated_redirect_table`
- 现有 `memory_*` / `tool_call_history` / `db` / `task_queue` 大量 round-trip 测试不受影响

## 后续可继续（非本次范围）

- `memory_index.yaml` 彻底剥离 butler_tasks / todo / task_archive 段：reads 已 SQLite-truth，但写仍走 yaml + mirror；从 yaml 完全砍掉需要 caller 侧逐处审计（panel rename / detail.md 文件路径管理 / consolidate 老段清扫）。
- LLM 端为 `daily_plan` / `persona_summary` 单值条目拆专用工具（`daily_plan_set` 之类），把 ai_insights 域里"日报"性质条目从 memory_edit 独立出来。
- DebugApp「🛠 专用工具占比」chip 增加 `task_archive` 列（现已被拒绝，看是否真有 LLM 误触）。
