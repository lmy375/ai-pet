# 当日 motion chip — 显示过滤后命中数（Iter R85）

> 对应需求（来自 docs/TODO.md）：
> mood entry 列表显示当前 motion 过滤命中数：DayMotionChips 行选中某 motion 后，应在 chip 旁附 `(N 命中)`，让用户对照"当日 N 条 vs filter 后 N 条"。

## 目标

PanelPersona 的当日详情有两层 entry 过滤：
1. motion chip（Tap / Flick / Flick3 / Idle / 全部）
2. 文字搜索框 `entrySearch`

两者叠加后会进一步缩小可见列表，但 chip 上只显当 motion 的总数（如 "Tap 15"），
用户搜了文字之后，无法在 chip 行直观看到"实际还剩几条"——必须往下数行。

本轮：当 motion 被选中且 `hits` 与该 motion 总数不同（即 `entrySearch` 又
narrowed 了一档），在 active chip 标签后附 `(N 命中)`。

视觉对照：
- "全部 30" chip = 当日 N 条
- "Tap 15 (8 命中)" chip = 选中 Tap 后，叠加搜索仅剩 8 条

## 非目标

- **`hits === counts[selected]` 时不附** —— motion-only 过滤、search 为空时
  `hits === counts.Tap === 15`，再写一次 "(15 命中)" 等于把 15 数字重复
  贴一遍，视觉冗余、占行宽。
- **不在"全部"chip 上附** —— TODO 明确"选中某 motion 后"。`selected === null`
  + 仅 entrySearch 缩窄的场景属于次要诉求，先不扩展。
- 不动 `MotionFilterChips`（sparkline 上方那行）—— 那里没有 entrySearch
  叠加，本身没有 hits ≠ count 的情况。

## 设计

### Props 改动

`DayMotionChips` 加 `hits: number` 必填 prop（=`visibleEntries.length`）。

### 渲染

```tsx
{present.map((m) => {
  const cnt = counts[m] ?? 0;
  const isActive = selected === m;
  const hitsSuffix = isActive && hits !== cnt ? ` (${hits} 命中)` : "";
  return <ChipButton label={`${glyph}${m} ${cnt}${hitsSuffix}`} ... />;
})}
```

### 调用点

PanelPersona.tsx 里：
```tsx
<DayMotionChips
  counts={dayMotionCounts}
  total={dayEntries.length}
  selected={entryFilter}
  hits={visibleEntries.length}
  onChange={setEntryFilter}
/>
```

### 测试

无单测；手测：
- 当日 30 条，Tap=15。点 Tap → "Tap 15"（无后缀）
- 再搜 "hello"，命中 8 → "Tap 15 (8 命中)"
- 清空搜索 → 后缀消失
- 搜 "xxxxx"（命中 0）→ "Tap 15 (0 命中)" — 0 命中也展示，告诉用户"过滤太严"

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | DayMotionChips + 调用点 |
| **M2** | tsc + build |

## 复用清单

- 既有 `visibleEntries` useMemo
- 既有 ChipButton

## 进度日志

- 2026-05-08 08:00 — 创建本文档；准备 M1。
- 2026-05-08 08:05 — M1 完成。`DayMotionChips` 加 `hits` 必填 prop；present.map 内取 `cnt = counts[m] ?? 0`、`isActive = selected === m`，仅 `isActive && hits !== cnt` 时拼后缀 ` (N 命中)`。调用点传 `hits={visibleEntries.length}`。
- 2026-05-08 08:08 — M2 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (499 modules, 963ms)。归档至 done。
