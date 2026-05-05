# 任务标签筛选 — 开发计划

> 对应需求（来自 docs/TODO.md「已确认」）：
> 任务标签筛选：面板任务列表顶部加 #tag chip 列表，点击仅显示带该 tag 的任务，按主题快速定位历史工作。

## 目标

接续上一轮的 `任务-记忆联动`（已让任务带 #tag）+ `任务搜索`（关键字过滤），把面板「任务」标签页的筛选能力补完整：从任务集合里抽出全部 tag，做成可点击 chip 列表；用户点击 chip 即把视图过滤到只含该 tag 的任务，多次点击形成"或"集合（任一选中 tag 命中即通过）。再点同一 chip 取消选中。

价值：当用户用 `#organize` `#weekly` `#文件整理` 等给任务打了几个月主题之后，可以一键回看"所有 #organize 相关的工作"。

## 非目标

- 不做"与（AND）"组合筛选 —— 多 tag 选中意味着"或（OR）"，与多数 chip 选择器交互一致。AND 增加学习成本，价值不显著。
- 不做"!#tag 排除"语法 —— 当前 tag 集合典型 < 20 条，正向选择已足够。
- 不持久化选中状态到 settings —— 切换标签页 / 刷新即重置；筛选是会话内动作。
- 不做后端 tag 索引 —— 任务列表前端已全量在内存，filter chip 集合遍历一遍即可。

## 设计

### 状态

```ts
const [selectedTags, setSelectedTags] = useState<Set<string>>(new Set());
```

### 派生：所有出现过的 tag

```ts
const allTags = useMemo(() => {
  const counts = new Map<string, number>();
  for (const t of tasks) {
    for (const tag of t.tags) {
      counts.set(tag, (counts.get(tag) ?? 0) + 1);
    }
  }
  // 按 count 降序、count 同则字典序升序
  return [...counts.entries()].sort((a, b) =>
    b[1] - a[1] || a[0].localeCompare(b[0])
  );
}, [tasks]);
```

### 过滤组合

`visibleTasks` 的 filter 链再加一层（紧接 search 之后）：

```ts
.filter((t) => {
  if (selectedTags.size === 0) return true;
  return t.tags.some((tag) => selectedTags.has(tag));
})
```

三层语义清晰：status → search → tag。

### UI

在 `searchRow` 下方加 `tagFilterRow`：水平 chip 行，每个 chip 显示 `#name (count)`。状态：
- 未选中：浅灰色背景 `#f1f5f9` + 灰文 `#475569`（与已有 tagChip 一致）
- 选中：靛紫背景 `#c7d2fe` + 深紫文 `#3730a3`，旁边带一个 ✓
- 点击切换；hover 变深一档

`allTags.length === 0` → 整行不渲染（不占空白）。

### 空态文案合流

把上一轮的"搜索无命中"扩展为"任意筛选条件无命中"：当 `trimmedSearch` 或 `selectedTags.size > 0` 任一非空且 `visibleTasks.length === 0` → 「没有匹配筛选条件的任务」。否则保持原有"已结束 / 进行中段无任务"语义。

### 任务行 chip 上的小升级

任务行已经渲染 tag chip。点击行内 chip 也加入 / 移除 `selectedTags`，让"在某条任务上看到 tag → 立即筛选"零跳。

## 阶段划分

工作量小，单次完成。

| 步骤 | 范围 |
| --- | --- |
| 1 | PanelTasks 加 selectedTags state + allTags derive + filter 组合 + 顶部 chip 行 + 空态合流 + 行内 chip 可点击 |
| 2 | tsc 检查 + 收尾（README 不更新 — UX 改进，非亮点级） |

## 复用清单

- 现有 `s.tagChip` 样式（基于此派生选中态变体）
- `TaskView.tags`（已存在，由后端 `parse_task_tags` 填充）
- `useState` / `useMemo`（已用过）

## 进度日志

- 2026-05-04 22:00 — 创建本文档；准备开工。
- 2026-05-04 22:10 — 完成实现：
  - `PanelTasks.tsx` 加 `selectedTags: Set<string>` state；`allTags` 派生自 `tasks`（不是 `visibleTasks`，避免"筛掉一个 tag 后它的 chip 也消失"的状态死循环）；`filtersActive` 合流 search + tag 两路。
  - 顶部 tag 筛选行：`#tag (count)` chip，选中态靛紫底 + ✓ 前缀；点击 toggle；`selectedTags.size > 0` 时旁边显示「清除」按钮；`allTags.length === 0` 整行不渲染。
  - 任务行内的 tag chip 改为可点击：点击切换该 tag 的全局筛选选中态（标题展示选中态 + 鼠标悬停 tooltip）。让用户从一条任务的 tag 一键 pivot 到 tag 视图。
  - 空态文案合流为「没有匹配筛选条件的任务」，覆盖 search / tag / 两者并存场景。
  - 全前端 derive，无后端改动。`tsc --noEmit` 干净；`cargo test --lib` 787/787 不受影响。
  - README 不更新 — UX 改进。
  - TODO 已确认条目移除；本文件移入 done/。
