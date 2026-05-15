# 桌面 pet 窗口 逾期任务徽章

## 背景

PanelApp 顶部「任务」tab 已有逾期计数红点徽章（30s 轮询 `task_overdue_count`）—— 但要先打开面板才看到。日常多数时间用户只面对桌面 pet 窗：宠物形象 + ChatMini 历史 + ChatPanel 输入框，**逾期任务没有任何视觉提示**，全靠用户主动切到面板 / 等宠物主动提醒。

加一个小徽章在 pet 窗 Live2D 区，让用户一眼看到"有 N 条逾期"且能点开。

## 改动

`src/App.tsx`：

- 新增 `overdueCount` state + 60s 轮询 effect：
  - 启动拉一次 `invoke<number>("task_overdue_count")`
  - 后续每 60s（与 PanelApp 的 30s 不同 —— pet 窗轮询更稀疏：用户长期看到的视图，省 LLM/IPC 噪声）拉一次
  - 失败 → 静默 `console.warn`，count 保持上次值（避免闪 0）
- 渲染：在 Live2D 区（已有 `MoodWidget` / 收起 `▶|` 钮的同层）顶部**左**侧加一个小 pill：
  - 仅当 `overdueCount > 0 && !hidden` 时挂
  - 文案：`🔴 {n} 逾期`（n > 99 显 `99+`）
  - 样式：`var(--pet-tint-red-bg)` 背景 + `var(--pet-tint-red-fg)` 字 + 12px 圆角 + shadow-sm
  - 点击：`e.stopPropagation()` + `openPanel()` —— 与 ChatMini 的 ⛶ 按钮同一入口
  - title：`{n} 条任务已过期 · 点开面板看「任务」tab`

布局选 top-left 与右上角 collapse `▶|` 钮分开，避免误点 collapse；右下角 MoodWidget 不动。

## 不做

- 不在徽章里 inline 显示具体任务标题：太挤；用户的实际意图是"该处理了"→ 跳到面板看完整列表
- 不在 PanelApp 那边自动 pre-set due="overdue" filter：v1 让用户自己点「任务」tab，跨窗口 deeplink 是另一坨工作量
- 不在 hidden 状态时显徽章：pet 缩到桌边只剩 tab，那时显徽章占位无意义（窗口已经隐形）
- 不复用 PanelApp 的 30s 周期：那是 panel 在用户主动打开时才轮询；pet 窗常驻，60s 更省

## 验收

- `npx tsc --noEmit` ✅
- 制造一个 `due` 已过期且 status=pending 的任务 → 60s 内 pet 窗 Live2D 区出现红 pill `🔴 1 逾期`
- 点 pill → 面板窗口开启（已有 `open_panel` 行为）
- 无逾期任务时 pill 不显

## 完成

- [x] App.tsx state + polling + pill render
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
