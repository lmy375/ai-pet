# PanelMemory butler_tasks「🔀 切 every↔once」一键按钮（iter #295）

## Background

owner 时常想"把循环任务改成一次性"或反之（场景："standup 这周休假，只跑一
次"或"原 once: 5/20 反思发现要每天做"）。当前唯一路径：走 ✏️ 改 schedule
modal — 切 radio kind → 改 date → 改 time → 保存，4 步。

本迭代加 🔀 一键按钮（仅 every / once 两 kind 互换，every_weekdays /
deadline 仍走 modal）：
- `every → once`：用今 / 明 HH:MM 自动算 next-fire 时刻（今日 HH:MM 已过
  则跳明日），不必让 owner 自己选日期
- `once → every`：保 HH:MM 丢年月日，next-fire 走每日循环

## Changes

仅 `src/components/panel/PanelMemory.tsx`：

- 在 ✏️ 改 schedule 按钮之后插 🔀 按钮：
  - gate：`butler_tasks + parsed + kind ∈ {every, once}`
  - every → once：用 `new Date(today + HH + MM)` 算 candidate；若 ≤ now
    跳 +1 天；拼 `[once: YYYY-MM-DD HH:MM]`
  - once → every：拼 `[every: HH:MM]` 丢 year-month-day
  - 新 prefix + 原 `parsed.topic` 拼成 newDesc → `memory_edit("update")`
  - loadIndex + 3.5s toast `🔀 已切 every → once：[once: ...]`
  - tooltip 文案根据当前 kind 动态调（提示具体 HH:MM）

## Key design decisions

- **仅 every / once 互换**：every_weekdays（mask 复杂）/ deadline（多 1 个
  urgency 维度）走既有 ✏️ modal 重路径。本按钮只覆盖最常用 every / once
  二态切换 — owner 一键搞定。
- **every → once 自动算 next-fire**：减少决策疲劳。今日 HH:MM 已过 → 明
  日，符合 "我现在切，应该尽快跑一次" 直觉；想自选日期仍可走 ✏️。
- **复用 memory_edit("update") 同后端**：与 ✏️ modal save 路径同源；SQL
  mirror / butler_history hook 自动跟进。
- **emoji 🔀 复合双向箭头**：自文档 "kind 切换"语义；与既有 ✏️ 编辑 +
  📐 复制 schedule / ⏰ 复制 prefix 同 small-button 行内布局风格。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.25s)
