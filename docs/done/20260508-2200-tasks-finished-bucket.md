# PanelTasks 已完成视图按完成日期分桶（Iter R94）

> 对应需求（来自 docs/TODO.md）：
> PanelTasks 已完成视图按完成日期分桶：showFinished 切到历史时，done/cancelled 任务按"今天 / 昨天 / 本周 / 更早"分组渲染，配合完成率统计形成立体复盘视图。

## 目标

`showFinished=true` 时，列表把 done/cancelled 任务和 unfinished 混在一起按
sortMode 排序，时间感知差。配合 R89 的"今日完成 X · 近 7 天 Y"完成率统计，
分桶把"做完的事"按"今天 / 昨天 / 本周 / 更早"分组，让用户做"周复盘 / 反
省" 时一眼看到节奏分布。

## 非目标

- 不动 unfinished 任务的渲染顺序 —— 它们仍按 sortMode（queue / due）排
- showFinished=false 时不显示分桶 —— 没有 finished 任务可分
- 不引入"按月分桶 / 自定义日期范围" —— 4 桶已覆盖典型复盘窗口；过细分桶
  让短期交互（"今天我做了啥"）认知成本高
- 不动 dueFilter today/overdue 路径 —— 那两个 filter 自动剔除终态，没有
  finished 进入桶

## 设计

### 桶语义

按本地时区计算：
- **今天**：本地午夜起（`setHours(0,0,0,0)`）
- **昨天**：昨天 00:00 起到今天 00:00 前（24h 前）
- **本周**：本 ISO 周一 00:00 起到昨天 00:00 前（即周一到前天）
- **更早**：本周一 00:00 之前

ISO 周（Mon=0）：JS `Date.getDay()` Sunday=0，转换为周一为本周第一天的语
义需要 `(d + 6) % 7` 偏移：
```ts
const dow = nowDate.getDay();
const isoOffset = dow === 0 ? 6 : dow - 1;  // 周日 → 6，周一 → 0...
const weekStart = new Date(nowDate);
weekStart.setDate(weekStart.getDate() - isoOffset);
weekStart.setHours(0, 0, 0, 0);
```

### 排序变化

把 `visibleTasks` 拆成 unfinished + finished 两段：
- unfinished 应用现有 sortMode（queue / due）
- finished 始终按 `updated_at` 降序（终态后 updated_at 即"完成时刻"，桶内
  天然时间序）

```ts
const unfinishedRaw = filteredTasks.filter((t) => !isFinished(t.status));
const finishedRaw = filteredTasks.filter((t) => isFinished(t.status));
const sortedUnfinished =
  sortMode === "due"
    ? unfinishedRaw.slice().sort(byDue)
    : unfinishedRaw;
const sortedFinished = finishedRaw.slice().sort((a, b) => {
  const ta = Date.parse(a.updated_at) || 0;
  const tb = Date.parse(b.updated_at) || 0;
  return tb - ta;
});
const visibleTasks = [...sortedUnfinished, ...sortedFinished];
```

### 桶 helper

```ts
type FinishedBucket = "today" | "yesterday" | "week" | "earlier";
const BUCKET_LABELS: Record<FinishedBucket, string> = {
  today: "今天",
  yesterday: "昨天",
  week: "本周",
  earlier: "更早",
};

function bucketFor(
  ts: number,
  todayMs: number,
  yesterdayMs: number,
  weekStartMs: number,
): FinishedBucket {
  if (ts >= todayMs) return "today";
  if (ts >= yesterdayMs) return "yesterday";
  if (ts >= weekStartMs) return "week";
  return "earlier";
}
```

### 渲染：注入分桶 subheader

`visibleTasks.map((t, idx))` 内：
```ts
const isFin = isFinished(t.status);
const prevTask = idx > 0 ? visibleTasks[idx - 1] : null;
const prevFin = prevTask ? isFinished(prevTask.status) : false;
const curBucket = isFin ? bucketFor(Date.parse(t.updated_at) || 0, ...) : null;
const prevBucket = prevFin ? bucketFor(Date.parse(prevTask!.updated_at) || 0, ...) : null;
const showHeader = isFin && curBucket !== prevBucket;
```

`<Fragment key=...>{showHeader && <SubHeader>}{TaskCard}</Fragment>`

SubHeader 视觉：

```tsx
<div style={s.bucketHeader}>
  <span>{BUCKET_LABELS[curBucket]}</span>
  <span style={{ marginLeft: 8, color: "var(--pet-color-muted)", fontWeight: 400 }}>
    {countInBucket}
  </span>
</div>
```

style：
```ts
bucketHeader: {
  fontSize: 12,
  fontWeight: 600,
  color: "var(--pet-color-fg)",
  marginTop: 12,
  marginBottom: 4,
  paddingBottom: 4,
  borderBottom: "1px dashed var(--pet-color-border)",
},
```

`countInBucket`：单次预扫 sortedFinished 算每桶数量（`Map<bucket, number>`），
header 时直接读，避免 map 内做 O(n²) 计数。

### keyboard 导航

`data-task-idx={idx}` 仍按 visibleTasks 数组下标走，subheader 不带这个 attr，
焦点 / scrollIntoView / `focusedIdx` 全部不受影响。

### 测试

无单测；手测：
- showFinished=false → 与原行为一致，不出 subheader
- showFinished=true 且没 finished → 不出 subheader
- 完成 1 条任务（updated_at = now）→ 列表底部出 "今天 1" 段
- 完成 1 条 + 1 条昨天完成 → "今天 1" / "昨天 1" 两段
- 跨周边界（周一早晨）→ 上周末完成的进 "更早" 桶

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | helper + visibleTasks 拆分 + 桶计数预扫 |
| **M2** | render 内 subheader 注入 + style 加 bucketHeader |
| **M3** | tsc + build |

## 复用清单

- 既有 `nowMs` state（30s tick）
- 既有 `isFinished` 守卫
- 既有 `s.section` / `s.item` 样式

## 进度日志

- 2026-05-08 22:00 — 创建本文档；准备 M1。
- 2026-05-08 22:15 — M1 完成。模块顶加 `FinishedBucket` type / `BUCKET_LABELS` const / `bucketFor` helper；visibleTasks 重构为 `[...sortedUnfinished, ...sortedFinished]`（unfinished 仍 sortMode；finished 始终 updated_at desc）；`bucketBoundaries` + `bucketCounts` useMemo 单次预扫，依赖 sortedFinished + nowMs（30s tick 跨午夜自动滚动）。
- 2026-05-08 22:20 — M2 完成。`s` table 加 `bucketHeader` / `bucketCount` 样式；map 内 isFin / curBucket / prevBucket 三件套判断；`return <Fragment key>{showHeader && <SubHeader>}{taskCard}</Fragment>`；`Fragment` 加到 import；data-task-idx 仍按 idx 走，键盘导航不受影响。
- 2026-05-08 22:23 — M3 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (499 modules, 947ms)。归档至 done。
