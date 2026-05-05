# 任务面板批量复制为 markdown — 开发计划

> 对应需求（来自 docs/TODO.md）：
> 任务面板批量复制为 markdown：select 多条 → "复制选中" 按钮 → 拼成多个 `## title` 段，方便一次贴入周记 / Notion 中的 todo dump。

## 目标

PanelTasks 已支持单任务 "Copy as MD" + select 多条的批量动作（重试 / 取消 /
改 due / 改优先级）。但用户想把"今天我有这 N 条 todo"作为列表 dump 到笔记
里时，要逐条点 Copy，重复 N 次。本轮在 bulk action toolbar 加 "复制为 MD"
按钮：把选中任务依次拼成 `## title` 段，一次写到剪贴板。

## 非目标

- 不要"周报"语义 —— 这是用户主动触发的 dump 工具，不是定期自动汇总报告
  （遵守 GOAL.md "不要周报日报相关需求" 约束）。
- 不带 detail.md 进度笔记 —— 批量场景下用户多半要的是"清单 view"而非
  每条详细内容；详情用单条 Copy as MD。这也避免 N 次 task_get_detail
  invoke 的延迟。
- 不带 history 时间线 —— 同理，单条详情才需要。

## 设计

### 复用 `formatTaskAsMarkdown` 但放宽 detail 参数

现签名 `formatTaskAsMarkdown(t: TaskView, detail: TaskDetail): string`。
让 detail 变可选（`detail?: TaskDetail`）：
- detail undefined → 跳过 "进度笔记" 段（`detail.detail_md` 那块）
- 其它字段（status / priority / due / tags / created_at / updated_at /
  body / result）全在 TaskView 里，本来就够

单任务 caller 仍传 detail，行为不变（防回归）。

### handleBulkCopyAsMd

```ts
const handleBulkCopyAsMd = useCallback(async () => {
  const titleToTask = new Map(tasks.map((t) => [t.title, t]));
  const parts: string[] = [];
  for (const title of selected) {
    const t = titleToTask.get(title);
    if (!t) continue;  // 选中后任务被删的边界 — 跳过
    parts.push(formatTaskAsMarkdown(t));
  }
  if (parts.length === 0) {
    setBulkResultMsg("无可复制任务（选中已被清掉）");
    setTimeout(() => setBulkResultMsg(""), 4000);
    return;
  }
  const text = parts.join("\n\n");
  try {
    await navigator.clipboard.writeText(text);
    setBulkResultMsg(`已复制 ${parts.length} 条为 markdown 到剪贴板`);
  } catch (e) {
    setBulkResultMsg(`复制失败：${e}`);
  }
  setTimeout(() => setBulkResultMsg(""), 4000);
}, [selected, tasks]);
```

### UI

bulk toolbar 在 "改 due" 按钮**之后**、`<span flex:1>` 之前加 "复制为 MD"
按钮（与既有 bulk 按钮同样式）。一键操作不需要 sub-panel，无确认 step。

## 测试

PanelTasks IO 重容器；前端无 vitest。`formatTaskAsMarkdown` 改签名后单
任务路径仍要工作 — tsc 类型检查 + 手测。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | formatTaskAsMarkdown 改 detail 可选 + 跳过 detail_md 段 |
| **M2** | handleBulkCopyAsMd useCallback + 边界处理 |
| **M3** | bulk toolbar 加按钮 |
| **M4** | tsc + build + cleanup |

## 复用清单

- 既有 `formatTaskAsMarkdown` 文案模板
- 既有 `bulkResultMsg` 反馈通道（与 bulk retry / cancel 同源）
- 既有 bulk button 样式
- 既有 selected: Set + tasks 数组

## 进度日志

- 2026-05-07 17:00 — 创建本文档；准备 M1。
- 2026-05-07 17:10 — M1 完成。`formatTaskAsMarkdown` 改 `detail?: TaskDetail`；undefined 时跳过 "进度笔记" 段；单任务 caller 仍传 detail，行为不变。
- 2026-05-07 17:15 — M2 完成。`handleBulkCopyAsMd` useCallback：tasks 全量索引 → for selected → 拼段 → blank line join → clipboard.writeText；选中后被删 race 跳过；一条都没拼到 → "无可复制任务" 提示。
- 2026-05-07 17:20 — M3 完成。bulk toolbar 在「改 due」之后、`<span flex:1>` 之前插 "复制为 MD" 按钮，与 bulk 系列同样式；title 解释 "不含 detail.md 进度笔记"。
- 2026-05-07 17:25 — M4 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (498 modules, 955ms)。归档至 done。
