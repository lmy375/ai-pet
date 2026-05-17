# PanelMemory butler_tasks「📊 schedule 24h 分布」mini bar chip（iter #398）

## Background

PanelMemory butler_tasks 段已有「📊 字数」chip（总字符数）。但 owner
想看"我的 task 都集中在几点 fire"分布偏态时仍需逐条 grok schedule
prefix — 没 visual 直观。

本 iter 加 24 bar mini sparkline 风 chip：每条 scheduled task（含
[every:] / [once:] / [deadline:]）按 hour 字段聚合到 24 桶，bar
高 normalize 到 max。owner 一眼看 "早 9 扎堆 / 中午稀 / 晚上多
" 分布偏态。

与 iter #392 PanelTasks priority distribution chip 同视觉 pattern
（mini bar + faint 占位 + max 归一化），但单色（purple tint）—
24 列已视觉密度高，单色让数量差异（bar 高）主导信号。

## Changes

### `src/components/panel/PanelMemory.tsx`（butler_tasks 段头，~line 4194）

```tsx
{catKey === "butler_tasks" && (() => {
  const buckets = Array.from({ length: 24 }, () => 0);
  let scheduledCount = 0;
  const doneRe = /\[done(?:\s[^\]]*)?\]/;
  for (const it of cat.items) {
    if (doneRe.test(it.description)) continue;
    const p = parseButlerSchedule(it.description);
    if (!p) continue;
    const h = p.schedule.hour;
    if (h >= 0 && h <= 23) {
      buckets[h] += 1;
      scheduledCount += 1;
    }
  }
  if (scheduledCount === 0) return null;
  const max = Math.max(...buckets, 1);
  return (
    <span title={...} style={{ display: "inline-flex", alignItems: "flex-end", height: 22 }}>
      <span>📊</span>
      {buckets.map((count, h) => (
        <span style={{
          width: 3,
          height: `${count > 0 ? Math.max(15, (count / max) * 100) : 5}%`,
          background: count > 0 ? purple : faint,
          borderRadius: 1,
        }} />
      ))}
    </span>
  );
})()}
```

设计要点：
- **24 buckets 固定**：每条 schedule 都有 hour 字段（every / every_weekdays
  / once / deadline 4 种 kind 都含 hour）；空桶 faint 5% 占位让 24
  列视觉对齐
- **bar width 3px**（vs iter #392 的 4px）：24 列比 10 列密度高，
  缩窄 bar 让总宽近似（~80px 含 emoji + padding）
- **purple 单色**（vs iter #392 三段 muted/blue/rose）：24 列分段染
  色会让 owner 看 24 个色 chip 不知所云；单色更聚焦 height 差异
- **过滤 [done]**：仅计 pending — 已完成 schedule 不该入"未来 fire
  时刻分布"
- **scheduledCount === 0 不渲染**：与既有 priorityCounts chip 同
  策略 — 没数据不渲 dead chip
- **title attribute 多行 tooltip**：列每个非空 hour + count，hover
  看精确值（"00:00 — 1 条"等）

## Key design decisions

- **位置紧贴 📊 字数 chip 之后**：两 📊 chip 视觉成组（都是 butler_tasks
  段"宏观数据"chip），并排让 owner 心智一致
- **不抽 share component**：与 iter #392 PanelTasks priority chip
  视觉相似但常量不同（10 vs 24 bar、3 vs 4 width、单色 vs 三段），
  抽 helper 要传 4-5 参数，单点内联 60 行直白可读
- **flex-end alignment**：bar 底对齐让"高 bar 朝上长"自然 — 与
  spreadsheet bar chart / iter #392 同视觉 language
- **不显星期维度**：仅 hour 分布；星期维度（every_weekdays 还有 mask
  fields）值得另一个 7d×24h heatmap，但显当 chip strip 信号噪声大；
  本 iter 单维度更聚焦
- **不为单 fn 引 unit test runner**：rendering only，build pass +
  手测足够（验有 fire 时刻分布的 cat 看 chip 是否出现 + 桶高度合理）

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.24s)
- 后端无改动 — 复用既有 parseButlerSchedule frontend helper
