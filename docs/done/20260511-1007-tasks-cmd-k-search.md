# PanelTasks ⌘+K 跳搜索框

## 需求

useTaskKeyboardNav 已有 ⌘+F 和 `/` 聚焦 search。⌘+K 是 Slack / Linear / Cursor
/ VS Code 等的"全局搜索"主肌肉记忆，也加上去让两类用户都顺手。

## 实现

`src/components/panel/useTaskKeyboardNav.ts`：把 ⌘+F 守卫条件从 `key === "f"`
扩成 `key === "f" || key === "k"`，行为完全一致（preventDefault + focus +
select existing text）。

`src/components/panel/PanelTasks.tsx`：search input placeholder 文案加上 ⌘K。

`src/components/panel/KeyboardHelpOverlay.tsx`：快捷键速查也同步加 ⌘K，注释
说明三家厂商习惯。

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 任务 tab 任何地方 ⌘+K → 搜索框聚焦，已有内容被选中（按下立刻 typing 覆盖）
  - 焦点在 textarea / button 内仍然 work（守卫"放最前"）
  - ⌘+F / `/` / ⌘+K 三个键都收敛到同一行为
  - ? 快捷键帮助层显新文案

## 不在本轮范围

- 没把 ⌘+K 扩到全 panel（PanelChat / PanelMemory / PanelSettings）—— 三处有
  各自的 search input + 自己的 onKeyDown 局部处理，扩 ⌘+K 得每个 panel 各自
  加；TODO 范围只点了 PanelTasks，先 ship 这块。其它面板等需求自然显现再扩
