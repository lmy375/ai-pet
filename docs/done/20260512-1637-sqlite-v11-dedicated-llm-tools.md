# SQLite v11：LLM 专用工具 butler_task_edit + todo_edit

## 背景

GOAL 明确：

> LLM 通过专用工具读写各域，不再共用 memory_edit

v10 完成所有业务域 SQLite 化后，LLM 的工具入口仍是单一 `memory_edit(category, action, title, description, detail_content)` —— 让 LLM 在调用时必须先选 category 字符串再填语义参数，本质是 "LLM 仍把所有业务域当作 memory 一种" 的接口设计。

本轮加专用 surface：
- `butler_task_edit` —— 管家任务委托
- `todo_edit` —— 用户给自己的提醒

## 改动

### `src-tauri/src/tools/memory_tools.rs`

新增两个 `Tool` impl：

#### `ButlerTaskEditTool`（name = `"butler_task_edit"`）

- 字段：`action` (create/update/delete) / `title` / `description` (可选) / `detail_content` (可选)
- 内部直接走 `memory::memory_edit("...", "butler_tasks", ...)`，所以**所有现有路径全跟随**：
  - yaml 写
  - SQLite mirror 双写
  - butler_history 事件日志（execute/delete 时记）
- description 文案重写：聚焦 butler 域语义（schedule 前缀格式、status marker 用法、与 todo 的语义区分）

#### `TodoEditTool`（name = `"todo_edit"`）

- 与 butler_task_edit 平行；内部走 `memory_edit("...", "todo", ...)`
- description 文案聚焦提醒域：`[remind: YYYY-MM-DD HH:MM]` 前缀格式 / 与 butler 的语义对比

### `src-tauri/src/tools/registry.rs`

- import 加 `ButlerTaskEditTool / TodoEditTool`
- `BUILTIN_TOOL_NAMES` 加这两个 name（前端工具风险设置面板自动可见）
- `ToolRegistry::new` 的 Box list 加两个实例

## 不做

- 不删 `memory_edit`！LLM 在 prompt-engineering 之前还会用旧接口；保留 memory_edit 接受 butler_tasks / todo category 让现有调用继续 work
- 不动 LLM 上下文 prompt（SOUL.md 等）—— 引导用户/产品 owner 决定何时把 prompt 改成偏好新工具
- 不写 ai_insights_edit / persona_edit 等 —— ai_insights 还是"思考记录" 含义最贴近原 memory 语义，本轮不分裂；剩余 user_profile / general / task_archive 同理

## 后续路径

1. 让 SOUL.md 提示词加："涉及管家任务用 butler_task_edit / 涉及用户提醒用 todo_edit；memory_edit 留给真正的记忆类条目"
2. 观察 LLM 切换情况（log 里两个新 tool 的调用占比）
3. 占比稳定后撤掉 `memory_edit` 的 butler_tasks / todo category 接受（让旧路径 fail-fast 强制升级）

## 验收

- `cargo build --release` ✅
- `cargo test --lib` ✅ 全 **880 通过**
- LLM 现在 tools 列表里多两个：
  - butler_task_edit
  - todo_edit
- 调用任一个都走 SQLite 双写路径（与 memory_edit 行为完全一致）
- 前端「设置 → 工具风险」面板能看到两个新名字（BUILTIN_TOOL_NAMES 列出）

## SQLite 流水线

- v0–v6 ✅ butler_tasks
- v7 ✅ todo
- v8 ✅ task_archive
- v9 ✅ kv_state + mood
- v10 ✅ ai_insights → kv
- v11 ✅ **专用 LLM 工具** —— GOAL "LLM 通过专用工具读写各域" 兑现

至此 GOAL 列出的 SQLite 分层重构核心要求全部实现：
- ✅ memory 只承担"大模型记忆/回想"职责
- ✅ butler_tasks / todo / task_archive / mood / 计划进度 等业务态搬出 memory，另建 sqlite 表
- ✅ LLM 通过专用工具读写各域（butler_task_edit / todo_edit；memory_edit 留给真正的记忆类）

## 完成

- [x] ButlerTaskEditTool / TodoEditTool 实现
- [x] registry.rs 注册 + BUILTIN_TOOL_NAMES 更新
- [x] 移到 docs/done/
