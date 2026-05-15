# PanelMemory 批量删除（勾选 + bulkBar）

## 背景

TODO：

> PanelMemory 批量删除：勾选 + 底部 action bar，bulk delete 记忆条目（与 PanelTasks 批量操作对偶）。

PanelMemory 当前只支持单条删除（hover 出按钮 + arm/confirm 二次确认）。consolidate 自动清理 + 手动单删足够日常运转，但偶尔遇到"今天 daily_review / mood event / tool history 一次性涌入十几条"的清理场景 —— 一条一条点 confirm 烦人。PanelTasks 已有批量操作（重试 / 标 done / 取消 / 改 priority / 改 due 全套），把同款对偶搬到 PanelMemory 是 ergonomic 补完。

## 改动

### `src/components/panel/PanelMemory.tsx`

**1. 选区 state + 操作 helpers**

```ts
const [selectedMemKeys, setSelectedMemKeys] = useState<Set<string>>(new Set());
const toggleMemSelected = (key: string) => { /* Set 操作 */ };
const clearMemSelection = () => setSelectedMemKeys(new Set());

const [bulkDeleteArmed, setBulkDeleteArmed] = useState(false);
const bulkDeleteArmTimer = useRef<...>(null);
const [bulkDeleting, setBulkDeleting] = useState(false);
const armBulkDelete = () => { /* arm 3s 自动 disarm */ };

const handleBulkDeleteMem = async () => {
  if (selectedMemKeys.size === 0) return;
  if (!bulkDeleteArmed) { armBulkDelete(); return; }
  /* disarm + setBulkDeleting(true) */
  for (const key of Array.from(selectedMemKeys)) {
    const sep = key.indexOf("::");
    const category = key.slice(0, sep);
    const title = key.slice(sep + 2);
    try { await invoke("memory_edit", { action: "delete", category, title }); ok++; }
    catch (e) { failures.push(`${title}: ${e}`); }
  }
  /* clearSelection + setMessage(...) + loadIndex + loadButlerHistory + setSearchResults(null) */
};
```

`key` 格式 `${category}::${title}` 与既有 `armedDeleteKey` 同模式 —— 跨 category 同名条目（理论极少但 ai_insights/daily_review 和 user_profile/daily_review 一类边界存在）不会碰撞。

逐条 `memory_edit("delete", ...)` 而非引入新 batch SQL 接口：保住既有 mirror 双写（butler_tasks → SQLite / ai_insights → kv_state）、detail.md 文件清理、butler_history 事件 audit trail。N < 几十条的批量量级，逐条调用开销可忽略；换 batch 要把这些一致性路径再 reimplement 一遍，代价不匹配。

部分成功有清晰文案：`"批量删除：成功 ${ok}，失败 ${failures.length}（title: reason; …）"`。所有成功 → `"已批量删除 ${ok} 条"`。

**2. Bulk action bar**

紧跟搜索行下方（consolidate 进度条上方），`selectedMemKeys.size > 0` 时浮出：

- accent 5% 底 + 40% accent 边色 + sm shadow，让"现在在批量选择模式"视觉清晰。
- 「已选 N 条」标签 + 「🗑 批量删除」(armed 时变红 + 「确认删除 N」) + 「取消选择」三按钮。
- arm/confirm 文案与单条 handleDelete 同：第一次点变红 + "再次点击确认（3s 后撤销）"，第二次点真删。

**3. Item checkbox**

每条 memory item 标题行左侧多了一个 native checkbox（`accentColor: var(--pet-color-accent)`）：

- click 切换自己的选中态，`stopPropagation` 防 bubble 到 row hover / 双击 navigation。
- `disabled` 在 rename in-flight 或 bulkDeleting 期间 —— 避免"正在编辑这条的同时勾选准备删"产生 UX 错位。
- `aria-label` 提供无障碍信息。
- 仅主列表渲染加 checkbox；search results 区暂不加（搜索结果是过滤视图，批量删搜索结果可能误伤跨 category 同名条；future 想加再说）。

## 不做

- **不加"全选当前 category"按钮**。一类目下 hundreds 条全选删除几乎一定是误操作；保留逐条点选作为有意识的动作。
- **不在搜索结果区加 checkbox**。搜索是"找一条"视图；批量删搜索结果语义模糊（"删 keyword 命中条"通常会误伤）。
- **不写 unit test**。前端无 vitest；handleBulkDeleteMem 是纯 IO 调度，逻辑简单（loop + try/catch）。
- **不引入 batch SQL 接口**。N 量级小，逐条调用保持既有 mirror / detail 清理 / audit trail 路径。
- **不动 PanelTasks**。它的 bulk 体系已存在且 cancel/done/retry/priority/due 五维都齐了，模式同步只在 PanelMemory 这一侧补完。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.25s
- 既有单条 handleDelete（armed/confirm）/ 单条 rename / handleConsolidate / handleExportAll 全部行为不变。

## 后续

- PanelMemory 顶部 "全选当前段" 小入口（如果用户表达需求）—— hover 加号下拉「全选 N · 选未 pin · 选今日 ≤ 30 天前」之类语义选择器，比纯"全选"更安全。
- 选中后 batch export to markdown（"导出选中"）—— 与既有「📋 导出」全量对偶；先看用户是否真有这个需求再加。
