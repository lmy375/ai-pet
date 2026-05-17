# PanelTasks「📊 priority distribution」mini sparkline chip（iter #392）

## Background

PanelTasks 已有 P0-P9 多选 chip 行（每条显数字），以及 priorityBands 三
段（P7+ / P4-P6 / P0-P3 各计 pending/done/...）。但 owner 想"一眼看
所有 priority 分布偏态"需扫整行 chip 数字 — 缺 visual 直观。

本 iter 加 sparkline 风 mini chip 行：10 bar（P0..P9）按 pending
count 比例显高度，color 跟 priorityBands 三段色族（muted/blue/rose）
对应。让 owner 看 "我的 task 都集中在 P3 还是 P7+？" 一眼分清。

## Changes

### `src/components/panel/PanelTasks.tsx`（priorityCounts chip row 之前，~line 7901）

```tsx
{priorityCounts.length > 0 && (() => {
  const buckets = Array.from({ length: 10 }, () => 0);
  for (const [p, count] of priorityCounts) {
    if (p >= 0 && p <= 9) buckets[p] = count;
  }
  const max = Math.max(...buckets, 1);
  const colorForP = (p: number) =>
    p >= 7 ? rose : p >= 4 ? blue : muted;
  return (
    <span title={...} style={{ display: "inline-flex", alignItems: "flex-end", height: 22 }}>
      <span>📊</span>
      {buckets.map((count, p) => (
        <span style={{
          width: 4,
          height: `${count > 0 ? Math.max(15, (count / max) * 100) : 5}%`,
          background: count > 0 ? colorForP(p) : faint,
          borderRadius: 1,
        }} />
      ))}
    </span>
  );
})()}
```

设计要点：
- **10 buckets 固定**：P0..P9 总是 10 列 — 即使部分 priority 0 也渲染
  细 faint bar 占位，让 chip 视觉横向稳定，owner 一眼看哪一列高
- **height % 而非 px**：用 flex 容器的固定 22px 高度作 base，每 bar
  按 max bucket count 比例缩放 — 视觉自适应 max
- **count > 0 时 min height 15%**：空 bar 5% 模糊占位、有 1 条的
  bar 至少 15%；防 max=10 时 1 条的 bar 仅 10% 太矮难分辨
- **color 三段（muted/blue/rose）**：与既有 priorityBands 三 chip
  色族一致（muted/var-blue/var-rose）— owner cross-surface 心智统一
- **`title` 多行 tooltip**：列每档具体数字，hover 看精确值
- **priorityCounts 空时不渲**：与既有 chip 行 `priorityCounts.length
  > 0` gate 同 — 没活动 task 整段 priority 区不显，本 chip 也跟随

## Key design decisions

- **位置紧贴 priorityCounts.map chip 行之前**：与 P{n} chip 同段
  视觉成组 — owner 看到 sparkline 后能直接点对应 P{n} chip filter
- **不抽 helper 共用 priorityBands 颜色**：colorForP 内联 3-line if/
  else 已最简；抽 helper 反而 imports 膨胀
- **bar width 4px / gap 2px**：经验值 — 10 bar + 9 gap + 📊 emoji
  + padding 总宽约 60px，与既有 chip 平均宽相当不抢空间
- **height: flex-end alignment**：bar 底对齐让"高的 bar 朝上长"自
  然 — 与 spreadsheet bar chart / DevTools heap profiler 同视觉
  language
- **不为单 fn 引 unit test runner**：rendering only，build pass +
  手测足够（验 P3 / P7 mixed 分布看 chip 是否显出 muted / rose 段）

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.24s)
- 后端无改动 — 复用既有 priorityCounts useMemo
