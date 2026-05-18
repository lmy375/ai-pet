# PanelMemory item「🔥 24h fresh」visual badge（iter #570）

## Background

PanelMemory item 既有 🔄 更新 X 前 chip 在 hover preview popover 内
（要 hover 才能看到）— ambient awareness 弱。owner 扫 cat 列表想识
「哪条最近动过」要逐条 hover 验证 — 低效。

iter #565 pivot 时已识：单 updated_at 字段无 history count，但二态
「最近动过 / 未动」是可达 visual signal。本 iter 实现这个 binary
freshness badge。

## Change

`PanelMemory.tsx` 在 item title row（item.title 渲染后、[silent] chip
前）加 always-visible 🔥 badge：

```tsx
{(() => {
  if (!item.updated_at) return null;
  const uMs = Date.parse(item.updated_at);
  if (isNaN(uMs)) return null;
  const ageMs = now.getTime() - uMs;
  if (ageMs < 0 || ageMs > 86_400_000) return null;
  const rel = ageMs < 60 * 60 * 1000
    ? "1h 内"
    : `${Math.floor(ageMs / 3_600_000)}h 前`;
  return (
    <span style={{
      fontSize: 10, padding: "1px 6px", borderRadius: 4,
      background: "var(--pet-tint-green-bg)",
      color: "var(--pet-tint-green-fg)",
      fontFamily: "'SF Mono', monospace", fontWeight: 500,
    }} title={...}>
      🔥 {rel}
    </span>
  );
})()}
```

## Key design decisions

- **二态显示（≤1h「1h 内」 / 1-24h 「N h 前」）**：信息密度内有变
  化但不噪杂；24h+ 直接不显（让 24h 边界成为「fresh」语义清晰边线）
- **always-visible（不 hover gate）**：与既有 hover-only 🔄 chip 互
  补 — 那个是 detail audit、本 badge 是 ambient scan signal
- **tint-green 配色**：与 stale 类 chip（💤 / ⏳ tint-red）错开。fresh
  ↔ stale 形成视觉对偶。绿色普世「健康 / 活动」语义
- **`now` 来自既有 component state（1s tick refresh）**：badge 准实时
  滚动（59min → 1h 内自动跳到「1h 前」）— 无需独立 timer
- **`fontWeight: 500`**：略加粗让 badge 在密集 item 列表中扫得到 —
  比 silent chip 的 muted gray 视觉权重高一档
- **不区分 cat / 全 cat 通用**：item.updated_at 是通用字段，所有 cat
  都有；不需 catKey gate
- **`updated_at` 解析失败兜底 null**：脏数据安全 — 不抛 React error
  也不显错 chip
- **`ageMs < 0`（updated_at 在未来）**：时钟回拨 / 数据脏兜底，跳过

## Verification

- `npx tsc --noEmit` clean
- 视觉手测 deferred — 单 chip 加在熟悉 item title row 位置（与
  既有 silent / reminderMin chip 同层），无 layout race

## Future iters (out of scope)

- **「⏳ 7d+ stale」配套 badge**：cold cousin — item.updated_at > 7d
  时浮 muted-red badge。形成 fresh / stale 双视角 ambient signal。但
  会让长期未动的老 task / archive item 全密集挂 — UI 噪音风险。propose
  时需评估
- **click-to-copy ISO**：本 badge 不可 click — 想复制 updated_at ISO
  仍走 hover preview 内 🔄 chip。click 入口加添 → 与既有 chip family
  pattern 一致，可作 follow-up
- **「N min 前」更细粒度**：60-min 内仅显「1h 内」；若 owner 想看
  「23 min 前」精度需扩展 sub-categorize。当前不显细粒度让 ambient
  signal 不分心
