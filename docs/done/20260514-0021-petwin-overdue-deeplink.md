# 桌面逾期 pill → Panel 任务 tab + overdue filter 一键直达

## 背景

上轮加了 pet 窗 Live2D 区左上角的「🔴 N 逾期」徽章，但点击后只 `openPanel()` —— 面板停在「聊天」tab，用户还得再点「任务」tab 并自己切 due filter 到 "overdue"。"v2: 跨窗口 deeplink" 当时挂的 TODO，本轮兑现。

## 改动

### 跨窗口 deeplink：localStorage 作信使

两套触达路径（panel 已开 vs 还没开）合并到同一信道：
- 写入：`localStorage.setItem("pet-panel-deeplink", JSON.stringify({ tab, dueFilter, ts: Date.now() }))`
- 已开 panel：`storage` 事件跨窗口触发，PanelApp 收到 → 读 + 清 + 应用
- 未开 panel：`open_panel` → PanelApp mount → useEffect 读 + 清 + 应用

`ts` 用作 TTL（10 秒内才认 —— 防止旧 deeplink 在用户后续手动开 panel 时误触发）。

### `src/App.tsx`：pill onClick 改写

替换原 `openPanel()` 单调用为：
1. `localStorage.setItem("pet-panel-deeplink", JSON.stringify({ tab: "任务", dueFilter: "overdue", ts: Date.now() }))`
2. `openPanel()` 不变

### `src/PanelApp.tsx`：consume deeplink

- 新增 `pendingDueFilter` state 透传给 PanelTasks
- mount effect 1：读 localStorage `pet-panel-deeplink` → TTL 内 → setActiveTab + setPendingDueFilter + 移除 key
- mount effect 2：`storage` 事件监听 → 同样的读 / 清 / 应用逻辑（保证已开 panel 在 pet 窗触发时也响应）

### `src/components/panel/PanelTasks.tsx`：consume prop

- `PanelTasksProps` 加 `pendingDueFilter?: "all" | "today" | "overdue" | "createdToday" | null` + `onConsumePendingDueFilter?: () => void`
- 挂载后 useEffect 消费：`setDueFilter(prop)` + `onConsumePendingDueFilter?.()` 清回 PanelApp 那边的 state，避免后续 filter 改回去再被 stale value 重设

## 不做

- 不为这次 deeplink 加专用 Tauri event：localStorage 已是跨窗口同步通道，多搭一层 emit/listen 反而要双源对齐
- 不支持任意 filter（tag / priority / origin）deeplink：本次只解决「逾期」这条最实际的跨窗口跳转；其它 filter 想加再扩 payload schema
- 不持久化 deeplink 后的 filter 状态：消费一次即清；用户在 panel 内改 filter 走原状态机不被 deeplink 干扰

## 验收

- `npx tsc --noEmit` ✅
- panel 未开时点 pet 窗 🔴 pill → 面板开 + 直接停在「任务」tab + overdue chip 高亮 + 列表只显逾期任务
- panel 已开（在「记忆」tab）时点 pet 窗 🔴 pill → 面板自动切到「任务」+ overdue filter
- 不点 pill 而手动打开 panel → 不受影响（deeplink 已 TTL 过期 / 不存在）

## 完成

- [x] App.tsx: pill onClick 写 localStorage
- [x] PanelApp.tsx: 双路径（mount + storage event）消费 deeplink
- [x] PanelTasks.tsx: pendingDueFilter prop + useEffect 消费
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
