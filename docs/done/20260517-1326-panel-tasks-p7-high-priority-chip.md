# PanelTasks 顶部「🎯 P7+ 高优」一键过滤 chip（iter #297）

## Background

owner 在 PanelTasks 上有两条常用聚焦动作：
1. 看 due 紧迫的 → 走顶部 dueFilter chip（今日 / overdue / createdToday）
2. 看高优 backlog 的 → 目前只能走 priorityFilter Set 多选 chip 行：手动
   勾 P7 / P8 / P9 三个 chip 才能聚焦"高优"

这第 2 条 friction 明显 — 「高优」是 owner 决策语义级别的常用动作，三次
点击不合理。本迭代加 🎯 一键 chip，单击即可 toggle "仅显 P7+ 高优"。

与既有 priorityFilter Set 互补：
- priorityFilter Set 是细颗粒挑选维度（"我就想看 P5"）
- 🎯 chip 是 owner 最常用的"高优 backlog 聚焦"快捷动作

## Changes

仅 `src/components/panel/PanelTasks.tsx`：

- 新增 state `highPriorityOnly: boolean`（localStorage 持久 key
  `pet-task-high-priority-only`，pattern 与 `pinnedFilter` 同源）
- 在 visibleTasks 过滤链 priorityFilter 之后插一条
  `.filter((t) => (highPriorityOnly ? t.priority >= 7 : true))`
- chip 渲染：插在 📌 pinned chip 之后、P{n} 多选 chip 行之前，鲜红
  rose tint 与中性 P{n} chip 区分；仅在 `priorityBands[0].pending > 0`
  时渲染（无 P7+ 活动 → chip 是 dead UI，不显）
- 显示 `🎯 P7+ {count}`，hover title 切换"已仅显 / 仅显"双语态
- `filtersActive` 加入 `|| highPriorityOnly` 让"清除全部"按钮 / 计数器
  在仅启用本 chip 时也亮起
- 4 个清 filter 入口（handleTaskRefClick / 完成小卡 jump-to-row /
  ✕ 全部 button / ✕ 清除全部过滤 button）都加 `setHighPriorityOnly(false)`
- 后两个清除按钮 tooltip 文案补 "/ P7+ 高优"

## Key design decisions

- **AND 语义而非 OR**：与既有 priorityFilter Set 两者都开时取交集
  （Set ∩ priority>=7）。owner 想"只看 P9"时勾 priorityFilter[9]，本 chip
  应继续生效（结果是 P9 — 仍 ⊆ P7+，符合直觉）；想"只看 P5"时同时开本
  chip 则交集为空 — 这种组合本就语义矛盾，让 UI 诚实表达"无匹配"比偷偷
  忽略一边更可教育。
- **priorityBands[0].pending 复用**：count 用已存在的 priorityBands 计数，
  零额外计算开销；与 PanelTasks 顶部"高优 N pending" 进度条同源数字 —
  owner 一眼对得上。
- **rose tint（鲜红）而非 amber / slate**：高优本身就是鲜亮信号；amber
  让给 📌 pinned（owner 自标注维度），slate / gray 让给 P{n}（结构化数字
  维度）。三色族错开识别更快。
- **localStorage 持久**：与 pinnedFilter 同 pattern — owner 打开后切走
  再回到面板状态保留。失败 fallback false 不打扰新用户。
- **空 chip 抑制**：`priorityBands[0].pending > 0` gate — 0 条 P7+ 活动
  任务时连 chip 都不渲染，避免"按了没反应"的死按钮。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.19s)
