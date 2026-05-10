# 精简宠物聊天首页按钮

> 对应需求（来自 docs/TODO.md）：
> 宠物聊天首页的按钮全部去掉，只保留一个输入框和一个点击就进入 Panel 聊天页的按钮。

## 范围

`src/components/ChatPanel.tsx` 当前显示在桌面宠物窗口底部的浮动栏 — textarea
+ 🔇 mute + 📝 note + AI 思考点 + ⚙ open-panel。需求要求只保留 textarea + 一个
进 Panel 聊天页的按钮。

## 设计

- 删除 🔇 mute 按钮（含右键 preset 菜单 / mute state polling）。Panel 设置里仍能改 proactive cooldown / 关闭 proactive。
- 删除 📝 transient note 按钮（含 popover）。Panel 设置面板可补回，先不做 UI。
- 删除 isLoading "thinking dots" — 是 indicator 不是 button，但题面要求"全部去掉"。改用 textarea placeholder 暗示 isLoading 即可（保留已有 disabled 行为）。
- ⚙ 按钮改为「打开 Panel 聊天」语义按钮：保持图标但 title 改成「打开聊天面板」，点击仍走 `onOpenPanel`。Panel 默认打开 settings tab，需 PanelApp 支持初始 tab —— 留给后续 iter；当下用户点完手动切到「聊天」即可。

## 删除产物

- `useState muted / showMenu / muted-related useEffects / handleMuteClick / handleMuteContextMenu / refreshMuteState / applyMute`
- `useState noteText / noteMinutes / noteActive / showNotePopover / note-related useEffects / handleNoteToggle / handleNoteSubmit / handleNoteClear`
- `pet-loading-dot` keyframes / `.pet-loading-dot` class / loading dots 渲染分支
- 整个 mute popover JSX + note popover JSX

## 风险

- Mute / note 是已有用户曾用过的功能，移除会让那条路径消失。但 GOAL.md 的方向是宠物管家执行型，UI 入口先收口；后续如果证明用户需要可以从 Panel 重做出来。
