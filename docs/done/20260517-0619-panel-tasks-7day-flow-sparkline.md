# PanelTasks「📈 7-day 任务流」sparkline（iter #258）

## Background

PanelTasks 已有"✅ 今日完成 N · 近 7 天 M" 汇总 chip，但只是总数 —— owner 看
不出 7 天里"做事的节奏"是稳定每天 2 条还是堆在某一天爆发后空白。视觉趋势对
"我最近规律变化了吗"这种自我检视问题极其有用。

本迭代在完成 chip 之后内联一个 7-day 双 stack bar sparkline：每天一列，上段
绿（新建）/ 下段蓝（完成）；hover 单 column 看精确数字 + 日期；hover 整个
chip 看 7 天累计。模板参考 App.tsx 既有"最近 7 天心情"sparkline（双击 widget
弹出），但更内联紧凑（不浮窗）。

## Changes

仅 `src/components/panel/PanelTasks.tsx`：

- **`flow7d` useMemo**：从 tasks 全集派生 `{ date, label, newCount, doneCount }[]`：
  - 7 天桶：day 0 = 6 天前 → day 6 = 今日（与视觉左→右最旧到最新一致）
  - `created_at` 落桶给 `newCount`；`status === "done" && updated_at` 落桶
    给 `doneCount`
  - `idxOfMs` 用 `Math.floor((ms - firstMs) / 86_400_000)` 简单分桶，依赖
    本地 0:00 边界（与 completionStats 同思路）

- **render**：在既有完成 chip 之后内联一个 80px 宽 chip，里面是 7 列：
  - 每列 width 6px，上下两 div 表示 new / done 计数
  - 高度归一化到 `max(newCount, doneCount)` 跨 7 天的最大值，最高 14px
  - 0 计数显示 1px 灰底（让"这天有 column 但没 task"可视区分于"没有这天"）
  - 整 chip tooltip 显累计 + 图例；单 column tooltip 显日期 + 精确数字

## Key design decisions

- **客户端计算而非新后端命令**：tasks 全集已含 created_at / status /
  updated_at，前端 reduce 一次 O(N) 即可；没必要新加 IPC。task_archive 段
  老 task 不算（设计如此 — 7 天窗口内的 task 不会被归档）。
- **双 stack 不是 +/- 镜像**：新建在上 / 完成在下而不是镜像零线，是因为绿
  与蓝色都"向上看着舒服"，零线镜像会让某些天的负值绿条造成视觉混淆。两段
  共享同一 max 归一化让"新建多" vs "完成多"比例直观。
- **0 计数 1px 灰底**：保留 column 框架让 7 天结构始终可见。owner 看出"周末
  停产"的模式时不会误以为这两天没有 column。
- **位置紧贴完成 chip**：两个 chip 都是"完成节奏"维度，相邻让 owner 一眼
  看汇总 + 趋势。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.22s)

## Notes

实际负载：7 天 × N tasks 的 reduce 单次 O(N) 计算，对 N < 1000 (实际通常 <
500) 不会成为渲染瓶颈。useMemo 仅依赖 `tasks` 引用，下次 reload 时重算。
