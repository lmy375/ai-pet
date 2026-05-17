# PanelTasks 任务行「📅 拖了 N 天」具体天数 hint（iter #320）

## Background

PanelTasks 任务行 due 已有 `dueUrgency()` 三态 chip（normal / soon /
overdue），overdue 时显「截止 YYYY-MM-DD HH:MM」+ 红 tint + tooltip 含
"已过期：X 小时前"。但 chip 文案是绝对时间，owner 想"这条拖了多久"得
hover tooltip 或心算 due 到 now 的差 — 不够 glance-able。

本迭代加专用「📅 拖了 N 天 / N 小时」chip — overdue ≥ 1 小时时显具体
拖延量。让 owner 一眼看「拖延量」决定"赶紧做 / 改 due / cancel"。

## Changes

仅 `src/components/panel/PanelTasks.tsx`：

- 既有「⏰ 还 N 分」未来倒计时 chip 之后插新「📅 拖了 N 天 / N 小时」
  chip：
  - gate：`t.due` 存在 + `!isFinished(t.status)` + `overdueMs >= 3_600_000`
    （< 1 小时由既有 dueUrgency overdue tooltip 已覆盖，chip 噪音）
  - 日 / 小时切换：`days >= 1` 显 `拖了 N 天`；否则显 `拖了 N 小时`
  - 红 tint 与 ⏰ 还 N 分 chip 同色族（时序对称：未来 = "还剩"，过去
    = "拖"，视觉一致）
  - tooltip 含 actionable hint「赶紧做 / 改 due / cancel」给 owner 决
    策方向

## Key design decisions

- **1 小时阈值**：< 1h 时既有 dueUrgency overdue chip 已经红 tint + tooltip
  "已过期 X 分钟前"覆盖（owner 直觉知道刚过去）。新 chip 触发后才有"拖
  延"语义。阈值再小（如 5 分）会与既有 chip 重叠制造视觉吵闹。
- **N 天 / N 小时 两档而非更细**：N 分钟级精度无意义（owner audit 拖延
  量时关心"天数 / 半天"量级，不关心几分几秒）。一天以上自动切天显避免
  "拖了 25 小时"这种不直觉数字。
- **与 ⏰ 还 N 分 chip 红 tint 同色族**：两 chip 是时序对称（未来 vs
  过去）—— 同色族让 owner 视觉建立"红 = 紧急时间信号"统一心智。orange
  / amber 留给 dueUrgency normal-overdue 三态主 chip 系。
- **tooltip actionable hint**：不只显数字，明确给三条 next-action
  建议（赶紧做 / 改 due / cancel）— 让 owner hover 即看到"该做啥"。
- **不依赖 useEffect / state**：完全 derive from `t.due` + `nowMs`（既有
  30s tick 自动驱动重渲）。chip 数字随 nowMs 更新自然刷新。
- **不引入 unit test**：纯 JSX 渲染逻辑（无 pure formatter 抽出）；行为
  通过 vite build + 真实交互验证。后续若 backend 需要"overdue 数量"
  会抽 helper 再补单测。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.22s)
