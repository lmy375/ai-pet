# pet 右键倒计时 preset 加 25 分标准番茄钟挡

## 背景

iter #216 加了 5/15/30 分倒计时三档。owner 反馈缺最常用的 25 分标准番茄钟 (Francesco Cirillo 1980s 经典 25 分 work + 5 分 break 循环)。

加 25 分 preset 让 owner 一键启标准番茄。

## 改动

### `src/App.tsx`

```diff
- {[5, 15, 30].map((m) => (
+ {[5, 15, 25, 30].map((m) => (
```

注释更新："五档" → "四档" + 新增 "25 分标准番茄钟" 说明。

menu H 经验值从 440 → 470 容纳新 button（+ 26px）。

## 关键设计

- **位置在 15 与 30 之间**：自然时长升序；25 紧贴 15 让 pomodoro / micro-pomodoro 邻近选项可比。
- **复用既有 startCountdownNudge logic**：array.map 模式 + 同 onClick 调用，无需新 handler。
- **H bump 30**：保留余量给字体放大 / 不同主题边距浮动。

## 不做

- **不让 25 分变 default high-light**：保持四档平权 — owner 偏好不同时长，pomodoro 主义者用 25，番茄分子论者可能用 15 等。
- **不写测试**：纯 array 加元素 + UI render；无逻辑分支。
- **不实现完整番茄循环 (25 + 5 break repeat)**：scope 单 iter 太重；本次只加 25 分单挡。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.19s
- 改动 ~10 行（preset 数组 + 注释 + H 调整）。既有 startCountdownNudge / countdownTimersRef / unmount cleanup 路径完全不动。

## TODO 状态

剩 5 条留池：
- PanelMemory ⌘K 唤起跨 cat memory quick-find palette
- PanelTasks 顶 chip 行加 "🎯 P0-P2 紧迫" chip
- ChatMini bubble hover 浮 "💾 转 task" 按钮
- detail.md 编辑器 toolbar 加 "🔍 detail 全文搜" 浮 search bar
- PanelTasks 列表行底加 "⏰ 还 N 分钟" 倒计时（due ≤ 60 分钟）

## 后续

- 25 分到点 +5 分 break 链：fire 时弹 "🍅 番茄结束！要不要起来活动 5 分？" 软消息 + 自动起 5 分倒计时（owner 配合手势确认）。
- 设置加 "我的常用倒计时"自定义 5 个 chip 让 owner 重排 / 自选时长。
- 加 long-break 模板 (4 番茄后 15 分长歇) 自动建议。
