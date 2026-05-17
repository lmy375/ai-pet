# PanelTasks「💤 全选 P0-P3 进 multi-select」chip（iter #362）

## Background

iter #359 加了「☑️ 全选 P7+」chip — 高优批量管理入口。本 iter
加对偶的「💤 全选 P0-P3」chip — 低优批量管理。owner 常见用例：
- 月底清理低优堆积：一键选全部 P0-P3 → 批量 cancel
- 改 tag：把 P0-P3 全标 `#later` 推迟
- 全部降到 P0：把 backlog 中暂不做的低优一次性塞到"idea 抽屉"

## Changes

### `src/components/panel/PanelTasks.tsx`

紧贴「☑️ 全选 P7+」chip 之后插：

```tsx
{priorityBands[2].pending > 0 && (() => {
  const lowTitles = tasks
    .filter((t) => t.priority <= 3 && t.status === "pending")
    .map((t) => t.title);
  const matchesLow =
    lowTitles.length > 0 &&
    selected.size === lowTitles.length &&
    lowTitles.every((tt) => selected.has(tt));
  const handle = () => {
    if (matchesLow) {
      setSelected(new Set());
      setBulkResultMsg("已清除 P0-P3 选区");
    } else {
      setSelected(new Set(lowTitles));
      setBulkResultMsg(`已选中 ${lowTitles.length} 条 P0-P3 进 multi-select`);
    }
    window.setTimeout(() => setBulkResultMsg(""), 2500);
  };
  return <span role="button" onClick={handle} ...>💤 全选 P0-P3</span>;
})()}
```

设计要点：
- **`priority <= 3` + `status === pending`**：与 priorityBands[2]
  (low: P0-P3) 一致，只选 pending（done/cancelled 不该被批量操作）
- **toggle 同模板**：selected 正好等于 P0-P3 集合时再点清空
- **muted/slate tint**：与 P7+ 的 rose tint 强对比 — owner 视觉上
  一眼分清"高优急 vs 低优休眠"。color-mix muted 12% bg + dashed
  border 让 inactive 态低分量，与"低优"语义自洽
- **glyph `💤`**：与"低优 / 休眠"语义对应（与 ☑️ 全选 P7+ 的
  checkbox tick 区分 — 都是 select 动作，但优先级 tier 不同）
- **`priorityBands[2].pending > 0` 渲染门槛**：与 P7+ 同策略，没
  P0-P3 pending 时不渲 dead chip

## Key design decisions

- **位置紧贴 P7+ chip 而非 P7+ 与 priorityCounts 之间**：两 chip
  都是"按 tier 全选"语义簇，并排让 owner 心智成组（high tier op /
  low tier op）。
- **不挂 🎯 P0-P3 filter chip 联动**：当前没有 P0-P3 filter chip
  (只有 highPriorityOnly = P7+)，添加一个会让 filter 行变拥挤。
  本 chip 仅 select 不动 view，与 P7+ chip 同职责分离原则。
- **抽 helper `selectAllAtTier(low|high|mid): void` 的诱惑**：
  两 chip 已重复了 ~30 行 IIFE。但抽 helper 会要传 (tier filter
  fn, label, glyph, tint vars...) 5+ 参数 + 2 个 setSelected
  / setBulkResultMsg 闭包 — 收益不抵复杂度。3 个 chip 再抽（如
  未来加 mid-tier P4-P6）。
- **不显数字 "💤 全选 P0-P3 (N)"**：P7+ chip 也不显数字（数字
  在 🎯 P7+ filter chip 上）。chip 行已经拥挤，重复数字噪音。
  title attribute 已含具体数量。

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.25s)
