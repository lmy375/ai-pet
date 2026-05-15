# 桌面 pet 窗 pill 加"✓ 今日完成"段

## 背景

上轮把"今日 stats"下沉到后端 `task_stats`。pet 窗的 pill 现在只显逾期（红色），少了正向激励 —— 用户做完事看不到"宠物记着你今天做了几件"。

加 done_today 维度到同一个 pill：胡萝卜（绿色 done 计数）与大棒（红色逾期计数）一起呈现。

## 改动

`src/App.tsx`：

### 数据源切换

- 删 `overdueCount: number` state + `invoke("task_overdue_count")` 轮询
- 改 `taskStats: { overdue: number; done_today: number } | null`，60s 轮询 `invoke("task_stats")`（取 5 字段中需要的 2 个）
- 失败仍保持上次值不闪 0

### Pill 渲染策略

```
overdue > 0 && done_today > 0   → "🔴 N · ✓ M"
overdue > 0 && done_today === 0 → "🔴 N 逾期"
overdue === 0 && done_today > 0 → "✓ M 今日完成"
both === 0                       → 不渲染
```

底色根据有无 overdue 切：
- 含逾期 → `var(--pet-tint-red-bg)` + 红色字（与之前一致，紧迫感优先）
- 仅 done → `var(--pet-tint-green-bg)` + 绿色字（庆祝色）

### 点击行为

- 含 overdue：保持原 deeplink → 「任务」tab + overdue filter
- 仅 done_today：deeplink 改 → 「任务」tab + dueFilter=all（让用户回看队列；不预设过滤）

复用既有 localStorage `pet-panel-deeplink` 协议，不动 PanelApp / PanelTasks 接口。

### title 文案

按状态拼：
- "{n} 条任务已过期 · 点开「任务」tab" 
- "今日完成 {m} 条 · 点开看队列"
- "{n} 条任务已过期 · 今日完成 {m} 条 · 点开「任务」tab"

## 不做

- 不上"昨日完成"对比：v1 先把"今日"维度做扎实；昨日是另一个数据源（task_archive 表筛日期），先看效果
- 不闪动画：单色 pill 已经够吸睛；动画会喧宾夺主分心 Live2D
- 不动 PanelApp 那边的 30s 轮询：那是 tab 红点徽章独立路径，与 pet 窗的数据流隔离没必要合并

## 验收

- `npx tsc --noEmit` ✅
- 无任务时无 pill
- 仅 done_today=2 → 绿色 pill 显 "✓ 2 今日完成"，点 → 「任务」tab + dueFilter=all
- 仅 overdue=1 → 红色 pill 显 "🔴 1 逾期"，点 → 「任务」tab + dueFilter=overdue（沿用之前行为）
- 同时 1 / 2 → 红色 pill 显 "🔴 1 · ✓ 2"，点 → 走 overdue 路径

## 完成

- [x] App.tsx: state + 轮询切到 task_stats
- [x] App.tsx: pill 渲染三态 + 点击 deeplink 二态
- [x] `npx tsc --noEmit` 通过
- [x] README 一行
- [x] 移到 docs/done/
