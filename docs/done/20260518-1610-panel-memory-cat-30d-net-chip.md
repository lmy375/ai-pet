# PanelMemory cat header「📊 30d 净增」chip（iter #576）

## Background

iter #555 已加 7d 净增 chip。完成 cat × period 矩阵：
- TG: /cat_growth_7d / /cat_growth_30d / /cat_decay_7d / /cat_decay_30d
- 桌面 PanelMemory cat header: 📊 7d ✓ / 📊 30d ← 本 iter

owner 月度复盘想看「本月持续投入这 cat 多少」单 chip 直见 — 比逐
PR 翻 commit 高效。

## Change

紧贴既有 7d chip 加 30d 兄弟 chip：

```tsx
{cat.items.length > 0 && (() => {
  const thirtyDaysAgoMs = now.getTime() - 30 * 24 * 60 * 60 * 1000;
  let delta = 0;
  for (const it of cat.items) {
    if (!it.created_at) continue;
    const cMs = Date.parse(it.created_at);
    if (isNaN(cMs)) continue;
    if (cMs >= thirtyDaysAgoMs) delta += 1;
  }
  if (delta === 0) return null;
  return (<button … >📊 30d +{delta}</button>);
})()}
```

## Key design decisions

- **与 7d chip 并排（不替换）**：让 owner 一眼比「本周 vs 本月」对比
  — 7d=5 / 30d=20 表示"持续投入 + 本周持平"；7d=5 / 30d=6 表示"本周
  爆发其他周静"；信号互补
- **0 delta 不渲染**：与 7d chip 同决策 — 死 cat 不挂 chip 噪音
- **click 复制单行**：与 7d chip 同 pattern；文案「<label> · 30d 净
  增 N 条」可直发同事 / 月报
- **tooltip 解释「7d vs 本 chip」对比意图**：避免 owner 看到俩 chip
  分不清差异 — tooltip 明示「7d 本周热点、30d 本月持续力度」
- **复用 `now` state（1s tick）**：与 7d chip 同 refresh — 不需独立
  timer

## Verification

- `npx tsc --noEmit` clean
- 视觉手测 deferred — 单 chip 加在 7d chip 旁，同 layout / 同 onClick
  pattern，无 race

## Future iters (out of scope)

- **`📊 90d / 365d`**：超长周期 chip — 季度 / 年度复盘信号。但 cat
  header 4 chip 已偏密；propose 时考虑收 dropdown
- **chip click context menu**：右键 chip 弹「7d / 30d / 90d / 自定义」
  picker — 让 owner 自由选周期 vs 当前固定 2 档。轻量交互改造
- **「7d / 30d 比率」mini sparkline**：单 cat trend chart inline 显
  daily new count over 30d — 比单数字信息密度高。需 backend bucket
  query；中型 lift
