# PanelTasks 列表头「🔥 streak N 天」chip（iter #345）

## Background

iter #339 ship 了 TG `/streak` 命令显本聊天连续 done 天数 + 7/30 天总
数。owner 在桌面端 PanelTasks 也想 glance 完成节奏 — 当前要切到 TG /
打开 chat → 发 /streak → 看 reply 才能拿到 streak 数字。本迭代加桌面对
偶 chip。

## Changes

仅 `src/components/panel/PanelTasks.tsx`：

- 新 useMemo `doneStreak`（在既有 `completionStats` 之后）：
  - 走 `Set<string>` 收集 done 任务的 updated_at 当日（`toLocaleDate
    String("sv-SE")` 拿本地 ISO YYYY-MM-DD — 与 PanelMemory 今日新
    增 chip / 1 / `todayNewCount` 同算法不漂移）
  - streak 末端：今日有 → today；否则若昨日有 → yesterday；否则 0
    （与后端 `compute_done_streak` 同语义）
  - 从末端 `new Date(${anchor}T00:00:00)` 往前 86_400_000ms 步循环数
    连续日
- chip 渲染在「✅ 今日完成 X · 近 7 天 Y」button 之后：
  - 仅 `doneStreak > 0` 时浮（0 时缺位表达"还没 streak"避免噪音）
  - Rose tint（与 PanelTasks 🎯 P7+ 高优 chip 同色族 — 与 streaks 应用
    "burn" 视觉一致）
  - 圆 pill 形 + bold 字 + 完整 tooltip 解释算法

## Key design decisions

- **toLocaleDateString("sv-SE")**：sv-SE locale 输出 ISO YYYY-MM-DD 但
  走本地时区。与既有 PanelMemory todayNewCount 同算法不漂移；后端
  Rust 走 NaiveDate parse_from_str("%Y-%m-%d") 与之等价（同 ISO 前缀
  10 字符 / 同本地午夜边界）。
- **streak = 0 时缺位（不渲染）**：与 PanelMemory 🌱 / 🆕 chip 同 gate
  on count > 0 pattern。owner 没 streak 时缺位本身即信号，不必显
  "streak 0 天"噪音 + 占位。
- **Rose tint 与既有 P7+ 高优 chip 共色族**：streak 是"激励 / 紧迫"
  信号 — 与"高优 backlog" 视觉色族对偶（rose / red shades 表示重要
  信号）。amber / blue / green 已被其它 chip 用。
- **不引入 unit test**：JS 算法与 Rust `compute_done_streak` 同语义，
  后者已有 6 unit tests 覆盖（empty / today-only / yesterday-only /
  3-consecutive / gap / no-anchor-zero）。前端是同算法 transpile —
  通过 cargo + vite build 验证一致；jsdom Date / toLocaleDateString
  在 jest 环境不稳。
- **不另发 backend Tauri 命令**：算法纯 client-side computable — task
  list 已经在 frontend，无需 IPC roundtrip。每次 tasks 变化 useMemo 自
  动 recompute。
- **不显 7 天 / 30 天计数**：那两个 stats 已在「✅ 今日完成 X · 近 7
  天 Y」button 显（component completionStats.week）；streak 是新维度，
  避免与既有 chip 数字重复。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.22s)
