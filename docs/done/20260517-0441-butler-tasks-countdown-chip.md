# PanelMemory butler_tasks 下次触发倒计时 chip（iter #249）

## Background

PanelMemory butler_tasks section 行内已有 "⏰ X 分后 / X 时后 / 已过 X 分"
显示，但：
1. 它是 inline 小灰字，没有 chip 视觉权重 — 与 scheduleLabel / silent / pinned
   等 chip 排在一行时缺乏对齐感
2. 它依赖 butler_history 15s 轮询触发的 re-render 顺带刷新，并不是真正的每分
   钟 tick — 当 owner 不在 butler section 上 hover、history 短期不变时（虽然
   实际 fetch 仍发生），UI 上的 "X 分后" 不会主动跨过分钟边界

owner 在「这条任务还多久跑？」的场景下希望扫读 chip 而不是依赖 inline 灰字，
本迭代两改一并到位：promote 成 chip 风格 + 加专用 60s 心跳。

## Changes

仅 `src/components/panel/PanelMemory.tsx`：

- **新增 `tickNow` state + 60s setInterval**：每分钟自增重渲，让所有依赖
  "当前时刻"的 inline `new Date()` 自动跨过分钟边界。在已有的 ⌘K palette
  state 块之前声明，集中和其它 panel-级 state 一起。

- **chip 内部 `const now = new Date()` 改为 `const now = tickNow`**：让倒计时
  随 60s 心跳 + butler_history 15s 轮询双触发 re-render（实际谁先到谁触发，
  另一个走 React reconciliation 跳过）。

- **`<span>` 样式 promote 成 chip**：加 `padding: 1px 6px / borderRadius: 4 /
  background`，与 scheduleLabel / silent chip 同视觉高度。
  - 未到点：`var(--pet-color-border)` 底色 + muted 灰字
  - 已过：`var(--pet-tint-orange-bg)` 底色 + orange 字（与 inline 已过 X 分
    时的 orange 字色一致，提醒 owner "宠物欠这条 fire 已"）

## Key design decisions

- **60s 而非 30s tick**：chip 精度只到分钟（"5 分后" / "12 时后"）；30s 心跳
  让 UI 半数 tick 没视觉变化，纯浪费 re-render。整分钟边界感与 owner 钟表读
  数同步即可。
- **不替换其它 inline `new Date()`**：useState ticker 仅给倒计时 chip 用；
  其它位置（如 created_at 相对时间 / butler_history age）变化粒度更粗（小时
  / 天）或本身用 lazy memo，引 60s ticker 反而增加无意义 re-render。
- **isPast 走 orange-bg 而不是 red-bg**：red-bg 已被 deadline `imminent` /
  `overdue` chip 占用；倒计时已过点是"提示而非紧急"，orange 介于 muted gray
  与 red 之间合宜。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.20s)
