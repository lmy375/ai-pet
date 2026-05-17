# PanelMemory butler_tasks 段「✅ 完成率」mini chip（iter #298）

## Background

butler_tasks 段头部当前有几个 chip：item 数 / 7-day churn sparkline / 🔇
silent N / 💤 snooze N / 最近 X。owner 能感知"近期活跃节奏"（sparkline 看
最近 7 天 update 次数）和"队列结构"（silent / snooze 计数），但缺一个
"累计产出"信号——「我把多少 butler 项目处理完了？」

本迭代在 butler_tasks 专属 chip 行的 💤 之后插「✅ doneN/totalN」chip，
与 7-day churn 互补——那个看节奏，这个看产出率。

## Changes

仅 `src/components/panel/PanelMemory.tsx`：

- 在 `catKey === "butler_tasks"` IIFE 内，与 silentN / snoozeN 并列计算
  `doneN`（regex `/\[done(?:\s[^\]]*)?\]/` 与 task_queue::has_done_marker
  同语义 — 要求 `[done` 后紧跟 `]` 或空格 + 包含闭合 `]`）和 totalN
- 在 💤 chip render 之后加 ✅ chip：
  - `totalN > 0` 时渲染（空段不污染 chip 行）
  - 显 `✅ doneN/totalN`，pct 仅在 tooltip 展示
  - emerald tint 当 doneN > 0；neutral 当 doneN === 0（让 0 产出仍可见但
    不抢眼）
  - tooltip 释 done = `[done]` marker、与 7-day churn 互补语义

## Key design decisions

- **denominator 含 every-recurring 是有意为之**：recurring 项永远算 pending
  压低 pct，但语义上仍合理 ——「我有 N 条 standing reminder，X 条已 once-
  and-done」是有效信号。如果只算 once-style 反而隐藏了"我塞了多少持续任
  务"的事实。
- **emerald tint 区分既有色族**：silent 🔇 = neutral / accent，snooze
  💤 = blue tint，pinned 📌 = amber，high-pri 🎯 = rose。✅ 完成率走
  emerald (green) ——「完成」语义最直觉色。CSS var 退化 → 硬编 `#d1fae5`/
  `#047857` fallback。
- **chip 显示比例 vs 文案**：选 `doneN/totalN` 分数而非 `XX%` —— 让 owner
  立刻看到样本量。3/4 比 75% 更有"我具体处理了几条"的感受；纯 % 在小样
  本下（如 1/2 = 50%）会误导。pct 留 tooltip 给"看大概"的场景。
- **与 task_queue::has_done_marker 同语义**：前端 regex 与后端 byte-scan
  对齐 — `[done]` / `[done at=...]` 都算 done；`[done...` 没闭合或字面
  "done" 都不算。避免前后端读"done"的标准不一致导致 chip 数字怪。
- **不点击 / 不交互**：完成率是只读 hint 信号，不像 silent chip 那样是
  filter 入口（filter 一段 [done]-only 任务对 owner 价值不高 —— done 任
  务已经在 LLM proactive 队列外）。保持 chip 纯展示，避免误触。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.19s)
