# mood sparkline 当日 entry 列表按 motion 过滤 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> mood sparkline 当日 entry 列表支持按 motion 过滤：drill 出来的 entries 当下混排，加顶部 motion chips → 只看 Flick3 那种"焦虑了几次"。

## 目标

上一轮 (4400) 让 sparkline 柱可点 → 列出当日 mood_history 全部 entries。但当
天混合了 Tap / Flick / Flick3 / Idle 时，用户想"只看焦虑那几条"还得肉眼挑颜色
块。本轮在当日详情列表顶部加一行 motion filter chips（含计数），点 chip →
列表只渲染该 motion 的 entry；再点 / 点"全部" → 复位。

## 非目标

- 不动 sparkline 顶部的 selectedMotion（那是"7 天柱缩放到只看某情绪"，是
  另一个 axis 的 filter，互不影响）。
- 不持久化过滤态：换天 / 关闭当日详情都重置；存到 settings 价值低。
- 不做时间段过滤（"上午 / 下午"）—— 当日 entry 很少，没必要。

## 设计

`MoodSparkline` 加 `entryFilter: string | null` state：
- selectedDate 变化时 reset 为 null（useEffect 依赖里加 reset）。
- 在当日 header 行下方插入一行 chips：
  - 计算 dayEntries 的 motion → count map（组件内 useMemo，单次 reduce）。
  - 渲染 "全部 N" + 有出现的 motion chips（按 MOTION_META 顺序，保证显示稳定）。
  - selected 时填充 motion 颜色，否则边框样式（与 MotionFilterChips 同视觉语言）。
- 渲染 entries 时按 entryFilter 过滤；filtered 0 条时显示 "当日无 {motion} entry"。

不抽取共享 chip 组件 —— MotionFilterChips 走 7-day 全量轴；当日 chip 显示
的是 in-day count 且要"全部"按钮，复用反而要加 props 分支，规模上得不偿失。

## 测试

纯 UI 状态改动；前端无 vitest，靠 tsc + 手测。后端无变更。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | entryFilter state + reset on selectedDate change |
| **M2** | DayMotionChips 内联组件 + 计数 + 选中视觉 |
| **M3** | filter dayEntries + 空态文案 |
| **M4** | tsc + build + cleanup |

## 复用清单

- 既有 MOTION_META 配色 + glyph
- 既有 dayEntries fetch（无变更）
- 既有 当日详情容器布局

## 进度日志

- 2026-05-06 08:00 — 创建本文档；准备 M1。
- 2026-05-06 08:10 — M1 完成。`MoodSparkline` 加 `entryFilter` state；`selectedDate` 变化的 useEffect 头部 reset 为 null。
- 2026-05-06 08:20 — M2 完成。新增内联 `DayMotionChips` + `ChipButton` 组件；按 MOTION_META 顺序渲染本日出现过的 motion，selected 时填充 motion 颜色。仅 1 种 motion 时整行隐藏（无过滤价值）。
- 2026-05-06 08:25 — M3 完成。`visibleEntries` useMemo 按 entryFilter 过滤；filtered 0 显示 "当日无 {motion} entry" 兜底。
- 2026-05-06 08:30 — M4 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (498 modules, 986ms)。归档至 done。
