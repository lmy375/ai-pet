# PanelTasks 🔒 blocker chip tooltip 加 blocker status（iter #303）

## Background

PanelTasks 任务行已有 🔒 等 <name> +N chip 显仍卡着的 blocker（iter
Cβ）。但 tooltip 仅列 blocker 标题，不显 status —— owner 想 audit "这条
blocker 是 pending 等执行，还是卡在 error 应该先 retry" 时只能展开 / 跳
到 blocker 行去看 status。

TODO 项: 「PanelTasks 任务行加「🔗 blocker N」inline chip：[blockedBy:]
锁数量直显（不必展开 expand 才看到），hover 列具体 blocker titles +
**status**」—— inline chip 已存在；本迭代补缺 tooltip 的 status 维度，
让 owner 不必展开两条 task 就能判断"怎么解锁"。

## Changes

仅 `src/components/panel/PanelTasks.tsx`：

- `computeUnresolvedBlockers` 返回 `Map<string, UnresolvedBlocker[]>`
  而非 `Map<string, string[]>`。新 `UnresolvedBlocker = { title, status }`
  内部 export 类型。同步收集 statusByTitle map 替代 activeTitles set
- `blockedMap` useMemo 类型同步
- 🔒 chip render 改用 blocker.title 取 preview / 计数；tooltip
  - 每条 blocker 前缀 ⏳（pending）/ ⚠️（error）emoji
  - tooltip 头根据 errorN > 0 切换文案：有 error blocker 时建议先
    /retry，否则维持既有"等下列任务完成或取消"文案

## Key design decisions

- **chip 保留 🔒 emoji 不改 🔗**：TODO 文字是 🔗 但既有 chip 用 🔒（lock
  语义贴合"被锁住"）。owner 已习惯 🔒；本次重点是 tooltip 加 status，
  不引入视觉 churn。
- **改 Map 值类型而非平行 lookup**：每行 render 时再 lookup blocker
  status 需要 O(blockers * tasks) 扫描 / 额外索引；直接在
  computeUnresolvedBlockers 里收集 status（同样 O(n)）更紧凑。export
  UnresolvedBlocker 类型也方便后续 chip / 排序逻辑复用。
- **errorN > 0 时切 tooltip 头**：把 "你应该先 /retry 那条 error blocker"
  这个 actionable hint 抬到 tooltip 头，owner hover 0.5s 即看到，避免
  漏过。否则保持既有"等完成或取消"中性文案。
- **emoji 选择 ⏳ / ⚠️**：与 PanelTasks 行内 status emoji 风格对齐
  （pending 主显 emoji 也是沙漏类，error 是 ⚠️）— owner 视觉 calibration
  一致。
- **未改 chip preview / count**：chip 文案 "🔒 等 X +N-1" 不变 — owner 一
  眼看 "等谁 + 几条卡着" 仍是首屏信号；status 在 tooltip 层是 progressive
  disclosure，避免 chip 越来越复杂。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.19s)
