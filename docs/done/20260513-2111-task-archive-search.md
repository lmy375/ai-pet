# task_archive 搜索框

## 背景

PanelTasks 切到「归档」tab 后，归档列表是一份只读 flat list，按 updated_at 倒序排。归档量随时间增长（consolidate 每 30 天把已结束的 butler_tasks 挪过来），需要在几十/几百条里翻找历史某条任务 ergonomic 不太好。

之前迭代里加了「📋 导出 MD」「刷新」按钮，但没有 in-place 搜索。

## 改动

`src/components/panel/PanelTasks.tsx`：

### 新 state

`archiveQuery: string` —— 搜索查询，session 内 live，不持久化（与队列 search 同 scope）。

### UI

在 `archiveExpanded && archiveItems.length > 0` 分支里：

- 搜索 input：placeholder 显总数 `搜归档 title / description…（共 N 条）`；非空时右侧显 `filtered/total` count + `✕` 清空按钮
- 过滤：query 非空时 `case-insensitive` 子串匹配 title || description
- 命中 0 条 → 渲染 EmptyState `🔍 没有匹配的归档`，hint `试试更短的关键词，或清空搜索看全集`

### 不动

- 不动 loading / empty 状态（无 archive 时不显搜索框）
- 不动月份分组导出 MD（仍走全集；后续若 owner 希望"只导出过滤结果"再加 toggle）
- 不持久化 query 到 localStorage

## 验收

- 切「归档」tab → 展开 → 顶部出现搜索框 + 总数 placeholder
- 输入关键词 → 列表实时过滤 + 计数刷新
- 命中 0 条 → 空态提示
- 清空 ✕ 按钮 → query 立即清空回全集
- `npx tsc --noEmit` ✅

## 完成

- [x] archiveQuery state
- [x] input + count + clear button + filter
- [x] empty state for 0 matches
- [x] 移到 docs/done/
