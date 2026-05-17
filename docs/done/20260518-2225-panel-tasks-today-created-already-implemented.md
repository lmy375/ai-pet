# PanelTasks 顶部 toolbar 「📅 仅今日新建」filter chip — 已实现 pivot（iter #506）

## Discovery

本 TODO 项「PanelTasks 顶部 toolbar 「📅 仅今日新建」filter chip：仅显
created_at 今日的 task — sprint「我今天加了啥」audit」在加入 TODO 前已
实现。

定位：`src/components/panel/PanelTasks.tsx`

- **line 1015-1060**：`DueChipKind = "today" | "overdue" | "createdToday"`
  + DueChip 渲染（active 时蓝填充 + emoji `🆕 今日创建`）
- **line 1399**：`dueFilter` state 含 `createdToday` 选项
- **line 5594-5603**：filter 逻辑 — 按本地 today YYYY-MM-DD 前缀比对
  `t.created_at`，不分 status（done / cancelled 也显，让 owner 复盘
  当日处理的全套）
- **line 6379**：`createdTodayCount` memo 给 chip 显计数

提 TODO 时未充分 grep `createdToday` / `🆕 今日创建` 关键词。本 cycle 第
6 个 already-implemented pivot（与 #495 #498 #499 #500 #506 同 procedure
教训）。

## Decision

不再重复实现。TODO 项删除，本 doc 作记录。

## Verification

- 手测：PanelTasks 顶部 toolbar 看到「🆕 今日创建 (N)」chip → 点击 active
  → filter 仅显本日 created_at 命中 task → 再点恢复全显
- 无新代码 / 无新测试
