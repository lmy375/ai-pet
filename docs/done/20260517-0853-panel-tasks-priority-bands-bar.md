# PanelTasks 顶部「优先级 3 段进度条」（iter #272）

## Background

PanelTasks 已有 ✅ 今日完成 chip + 📈 7-day sparkline 显总体节奏，但 owner
扫读时无法快速判断"高优 vs 中优 vs 低优 这三个 backlog 的体量与进度"。tags
chip 行的 priorityCounts 数字只显未完成数，缺各段的 done / error 对比。

本迭代加 3 段堆叠 bar：高优 (P7-P9) / 中优 (P4-P6) / 低优 (P0-P3)，每段一
根 64×6 stack bar，颜色对应 pending 蓝 / error 红 / done 绿 / cancelled 灰，
宽度比例 = 各类 / 段内 total。tooltip 4 类精确数。

## Changes

仅 `src/components/panel/PanelTasks.tsx`：

- **`priorityBands` useMemo**：从 tasks 全集派生 3 个 Band：
  - high (P7-P9) / mid (P4-P6) / low (P0-P3)
  - 每段 4 个计数：pending / done / error / cancelled
  - 与 R107 / task_queue::compare_for_queue 的 "数值大 = 优先级高" 方向一致

- **render**：在 7-day sparkline 之后内联另一个 chip 容器：
  - 仅当至少一段 total > 0 时显（避免空 panel 时占位）
  - 每段 column = label("高优 12") + 64×6 stack bar
  - 段内 total === 0 的段跳过（filter visible）
  - tooltip 4 类精确数 + 段范围说明

## Key design decisions

- **3 段划分 P7+/P4-6/P0-3**：与 TaskHeader priority 0-9 范围 + R107 方向一致；
  3 档够 owner 扫读，更细（如 5 档）会让单 bar 太窄看不清比例。
- **堆叠 bar 而非分类 bar**：单个 bar 内 4 种颜色直接拼出 "段内完成率"
  视觉（done 绿段越长越好）。分类 bar 占 4 倍空间但只显独立段，节奏感丢。
- **空段隐藏整段**：owner 没用过 P7-P9 高优时（如新用户）整个高优段不显，
  保持 panel 简洁。three 段都空 → 整个 chip 容器不渲染。
- **cancelled 用 muted 灰 + opacity 0.5**：cancelled 是放弃态，与 done 不同；
  低饱和度提示"它在那但不算产出"。
- **bar 嵌在既有 dashed border 容器**：与 sparkline 同视觉风格（同
  border-radius / 同 dashed border / 同 verticalAlign middle）。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.20s)

## Notes

代码里另一处 `urgentTopPriorityCount` 把 P0-P2 当 "高优 backlog 信号"，与
R107 方向相反 — 看着是个历史遗留 bug，本迭代不修以免范围漂移。
