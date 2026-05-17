# ChatMini 顶部「🌡️ context 健康」mini progress bar（iter #290）

## Background

ChatMini 顶部仅在 sessionTokens > 4000 token 时浮一条警示 chip + /reset CTA。
但 owner 经常在不到 4000 前就好奇"我聊了多少了 / 离阈值多远"——突然弹警示
缺乏渐变的预期管理。

本迭代加常态进度条：sessionTokens 在 800 (20%) 到 4000 之间时显，bar 颜色按
< 50% / 50-75% / ≥ 75% 三档变化（绿 / amber / red）。> 4000 后由既有警示
chip 接力（同信号两层视觉权重 — bar 是 ambient peek，chip 是 CTA）。

## Changes

仅 `src/components/ChatMini.tsx`：

- 在既有警示 chip 之前插入新 mini bar 块：
  - gate：`sessionTokens > MINI_TOKEN_WARN_THRESHOLD * 0.2 && sessionTokens
    <= MINI_TOKEN_WARN_THRESHOLD` （避免空 session 噪音 + 撞警示线后让位）
  - 一行布局：🌡️ icon + 4px 高进度条 + `N/M` 数字
  - 进度条 fill 颜色：
    - `< 50%`（2000 以下）→ 绿
    - `50-75%`（2000-3000）→ amber
    - `≥ 75%`（3000+）→ red
  - tooltip 显完整 `${sessionTokens} / ${threshold} tokens (X%)` + 接力提示

## Key design decisions

- **20% lower bound 避免空 session 噪音**：刚起新对话时 sessionTokens 可
  能是 200-500（system prompt 等），bar 此时显反成"陪伴的眼睛"压力。20%
  阈（800）覆盖"已经在聊"区间。
- **撞警示线后让位 chip**：bar 是 ambient hint，chip 是 explicit CTA（含
  /reset 按钮）。同一信号两层视觉权重 — owner 临近时 peek bar 自我调整，
  到点后被 chip 强提醒。让位避免两条同时显堆叠占视觉位。
- **三档色阶**：与既有 priority 进度条 / 7-day sparkline 同色阶语言
  （绿 / amber / red 表"健康 → 注意 → 警示"），让 owner 在多个 chip 间
  色感一致。
- **行布局 vs 圆点 / 文字**：进度条比单数字更直观 — `1234/4000` 要心算，
  bar 一眼知道"接近一半 / 三分之二 / 快撞"。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.22s)
