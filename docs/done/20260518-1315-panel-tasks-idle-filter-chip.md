# PanelTasks 「💤 N 条 7d+ idle」批量 filter chip（iter #563）

## Background

iter #559 加了 row hover「💤 Nd 未动」chip — per-row inactivity 信号。
但「我所有 stale backlog 是哪几条」需逐行 hover audit — 低效。本 iter
加 panel-wide filter chip：一键聚焦 7d+ 未动的 pending task，与 row
chip 同 7d 阈值呼应。

## Change

新 idleFilter（与 pinnedFilter / highPriorityOnly 同 pattern）：

1. `idleFilter` boolean state + localStorage 持久（key
   `pet-task-idle-filter`），紧贴 `highPriorityOnly`
2. `idleCount` useMemo — pending + `updated_at ≤ now - 7d` 计数；
   memo 依赖 `tasks + nowMs`
3. filter pipeline 加 `.filter(idleFilter ? gate : true)` — 紧贴
   `pinnedFilter` 那一行（AND 叠加）
4. `filtersActive` 加 `|| idleFilter`（让既有 reset-filters 入口
   识别本 toggle）
5. 新 chip 在 chip-bar 紧贴 📌 pinned chip — `💤 N` red tint
   (active = red bg / inactive = red bg-tint)。click toggle
6. chip-bar 可见 gate `|| idleCount > 0`

## Key design decisions

- **7 天阈值与 hover chip 一致**：避免「chip 7d / filter 5d」不一致
  造成「row 没 chip 但 filter 列出来」错位
- **只数 pending**：done / cancelled / error 不在 inactivity 语义里
  — finished 任务"未动"是常态非问题。owner audit「我搁着没做的」时
  错误 task 也算 stale 吗？这是判断题；选择 pending only 让本 chip
  聚焦最常用 audit 维度，error 单独 chip 已经存在
- **`updated_at ≤ cutoff`（含等号）**：边界 exactly 7d 前的算 stale。
  天数比较，秒级精度不影响 owner 直觉
- **`updated_at` 而非 `created_at`**：与 hover chip 一致 — 「未动」
  是 inactivity since last activity，不是 age since creation
- **AND 叠加既有 pinnedFilter / highPriorityOnly**：filter 都是 AND
  按 pipeline 顺序串。owner 想看「stale 高优」开 idleFilter +
  highPriorityOnly；想看「stale pinned 钉了忘了的」开 idleFilter +
  pinnedFilter — 三态交集让 audit 灵活
- **red tint 配色与 row hover chip 统一**：active 时 fg/bg 互换、
  inactive 时 tint-bg + tint-fg border — 与 📌 amber 配色错开
  （pinned 是 owner 标注；idle 是 stale 警示，不同色族）
- **0 数不渲**：避免 dead chip 占位 — 与既有 pinnedCount / errorTaskCount
  / 其它 chip 同决策

## Verification

- `npx tsc --noEmit` clean
- 视觉手测 deferred — chip 加在熟悉位置（紧贴 📌 pinned），filter
  pipeline 一行，state 一模一样 pattern，无 layout race 或 filter
  pipeline 顺序敏感问题
- 无新 lib test — pure React state + filter chain

## Future iters (out of scope)

- **idle threshold 用户可配**：当前 hard-coded 7d；owner 若想 14d / 3d
  阈值需常量改。settings 入口加 dropdown — propose 后单独评估
- **idle bucket chip 系列**：分档「7-14d / 14-30d / 30d+」三 chip 让
  分级 audit；信息密度高但 UI 繁。先观察 owner 是否需要再做
- **「💤 N」chip 加 ⏰ 批量 snooze 入口**：filter open 后再点 chip 弹
  popover 一键给当前 visible idle batch 全 /snooze tomorrow — 让
  audit + action 一步到位。需 bulk action 路径
- **TG 端 `/idle_7d`**：远程同 audit；list pending idle task 含
  updated_at — 跟 pin_grow_7d / pinned_drop_7d 同思路按需 propose
