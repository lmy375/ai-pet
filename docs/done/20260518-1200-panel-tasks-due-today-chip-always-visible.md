# PanelTasks 「📅 today」filter chip 始终可见（iter #471）

## Background

PanelTasks 顶部既有「📅 今日到期 (N)」filter chip + dueFilter="today"
state — 点击切换"只看 due 在今天 + 未结束的 task"视图。但该 chip 被
`dueTodayCount > 0` 门控隐藏 — owner 在「当前还没安排 due 在今日的
task」时**完全看不到这个 filter 入口**。

TODO 提出「PanelTasks 顶部加「📅 due today」filter chip」— 作者可能
没意识到 chip 已存在（被 count > 0 gate 隐藏）。本 iter 做最少必要
修改 — 移除 count gate 让 chip 始终可见 — 让 filter 作为「今日必做」
视图的常驻入口。

## Changes

### `src/components/panel/PanelTasks.tsx`

#### 移除 `dueTodayCount > 0` 门控

```diff
-{dueTodayCount > 0 && (
-  <DueChip kind="today" count={dueTodayCount} active={dueFilter === "today"}
-           onToggle={...} />
-)}
+<DueChip kind="today" count={dueTodayCount} active={dueFilter === "today"}
+         onToggle={...} />
```

count=0 时 chip 自然显「📅 今日到期 (0)」— 视觉与 count>0 一致（不
muted），让 owner：
- **discover**：知道这是常驻 filter 入口
- **pre-set use case**：先 enable filter，之后新建 due 今日的 task 自
  动浮顶视图（不必每次新建后再切 filter）
- **toggle 反馈一致**：activate / deactivate 视觉与 overdue / createdToday
  chip 同协议

## Key design decisions

- **不引新 chip 组件**：DueChip 已 cover「today filter」语义；新组件
  只会是重复。最小变更最稳
- **count=0 不 muted**：与 overdue / createdToday count>0 渲染同样
  visual emphasis — 三 chips 视觉一致；用户从颜色一眼区分类型（红 / 橙
  / 蓝），count 数字提供精确信息
- **保留 overdue / createdToday count>0 gate**：这两 chip 信号强度依赖
  count（"有 N 条逾期"是 warning；"有 N 条今日新建"是 momentum）。
  count=0 时它们隐藏减视觉噪音。但「today filter」是中立工具 chip 而非
  状态指标，含义恒定，不需要 gate
- **不写 unit test**：纯 JSX gate 移除；既有 dueFilter 行为 / DueChip
  组件 / isDueToday 算法 production 验证。GOAL.md "meaningful tests
  only" 规则下不引装饰性测试
- **不动 dueFilter state 默认值 / 初始化逻辑**：filter 仍默认 "all"；
  chip 仅是 UI 可见性变更，不改 state machine 语义

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.28s)
- 后端无改动 — 纯前端 chip 可见性调整
- 手测：PanelTasks 在「今日无 due task」状态下 → 顶部仍能看「📅 今日
  到期 (0)」chip → click → list 空（"今日 0 条"自然）→ 再 click 退出
  filter 恢复全表；之后新建一条 due=tonight 18:00 task → list 立即显
  该条
