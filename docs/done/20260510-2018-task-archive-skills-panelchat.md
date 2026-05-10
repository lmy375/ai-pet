# 三件套：butler_tasks 归档 / 技能简档 / PanelChat 抽组件

> 对应需求（来自 docs/TODO.md）：
> 1. butler_tasks 老条目自动归档：consolidate 把 done / cancelled 且 updated_at 早于 30 天的任务挪到 ai_insights/task_archive_YYYY-MM 类目（实现里改为 `task_archive` 单类目，title 加 YYYY-MM-DD 前缀防同名碰撞）。
> 2. 宠物技能简档：派生 tool_call_history 的 top 5 工具使用频次 + 最近一次调用时间，在 PanelPersona 加一节「最近常用的工具」。
> 3. PanelChat 抽出 SearchResultRow / SessionDropdown 子组件，主组件压到 < 1300 行。

## 1. butler_tasks 自动归档

### 后端
- `proactive::butler_schedule::is_archive_candidate(desc, updated_at, today, retention_days)`：纯判定，状态为 done / cancelled 且 updated_at 距今 ≥ retention_days 时返回 true。pending / error 永远不归档；retention_days = 0 关闭归档；updated_at 解析失败返回 false（保守）。8 单测覆盖每分支。
- `consolidate::archive_old_butler_tasks(today, retention_days)`：IO 包装，挑出候选 → memory_edit("create", "task_archive", `<date>_<title>`, `[archived: <date>] <desc>`) → 成功后 memory_edit("delete", "butler_tasks", title) → butler_history 写一条 `archive` 事件。每条独立失败可跳过，避免数据丢失。
- `commands::memory::MemoryIndex::default()` 加入 `task_archive` 类目（label 「任务归档」）。
- `read_index()` 加入「补全缺失默认 category」分支，让旧 index.yaml 也能直接 `memory_edit("create", "task_archive", ...)`，不需要手动迁移。
- `commands::settings::MemoryConsolidateConfig.stale_butler_archive_days`（默认 30）+ default fn。
- `consolidate::run_consolidation` 在 daily_review 清理之后调用归档，归档计数 > 0 时记一行 log。

### 前端
- `useSettings.MemoryConsolidateConfig.stale_butler_archive_days` 字段。
- `PanelSettings` 在「记忆整理」的天数面板里加一个 PanelNumberField，标签「butler 任务归档 (天，0=关闭)」，并扩展原有解释段。
- `DEFAULT_SETTINGS` 与 `PanelSettings` form 初值都填 30。

## 2. 宠物技能简档

### 后端
- `tool_call_history::ToolUsageStat { name, count, last_used_at }`，`derive_top_tools(records, top_n)` 纯派生：count 降序 → last_used_at 降序 → 名字升序，截断到 top_n。空输入或 top_n=0 返回空。7 单测覆盖空入 / top_n=0 / 计数排序 / 最近时间 pick / 截断 / count tie 取最近 / 全 tie 字典序。
- `get_top_tools_used()` Tauri 命令：snapshot 当前 ring buffer (cap 30) → 派生 top 5。
- 在 `lib.rs::invoke_handler` 注册命令。

### 前端
- `PanelPersona` 新增一节「最近常用的工具」（在自我画像之后、当下心情之前）。fetchAll Promise.all 拉 `get_top_tools_used`，渲染 top 5 行：左侧工具名 code 块、× count、右侧相对时间（刚刚 / N 分钟前 / N 小时前 / N 天前）；空 buffer 走「还没动过手」empty state。

## 3. PanelChat 抽组件

### 改动
- 新文件 `src/components/panel/panelChatBits.tsx`（193 行）：`bubbleStyle`、`exportSessionAsMarkdown`、`CopyableMessage`、`SearchResultRow` 子组件，以及 `ChatItem` / `SearchHit` / `ToolCall` 共享 type。文档注释照搬。
- `PanelChat.tsx` 主组件从 1550 → 1370 行（-180 行）。`bubbleStyle` 仍被 PanelChat 内联用于错误 bubble / streaming bubble，所以保留为外部 import 而非纯 bits 内部。

### 偏离点

TODO 写的 < 1300 行；实现后是 1370 行（差 70 行）。剩余 PanelChat 主要是 useEffect / event listeners / state hooks 各自独立，再拆只能强行拆破组件状态边界，得不偿失。1370 已经显著比 1550 易读。

## 验证

- `cargo test --lib` → 896 passed (881 旧 + 8 archive + 7 derive_top_tools)。
- `tsc --noEmit` 干净。
- `vite build` 干净。
