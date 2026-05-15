# DebugApp `⌘1`–`⌘4` 跳 tab

## 背景

PanelApp 上轮加了 `⌘1` – `⌘5` 跳 tab。DebugApp 也是 4-tab 结构（"应用 / 日志 / LLM 日志 / 统计"）但缺这套快捷键，得用鼠标点。补齐让 panel / debug 两个窗口的 tab 切换肌肉记忆一致。

## 改动

`src/DebugApp.tsx`：加一个 keydown effect：

- `metaKey || ctrlKey` 且 `!shiftKey && !altKey`
- 键名 `"1"`–`"4"` → `setActiveTab(TABS[idx])`
- 守门：focus 在 INPUT / TEXTAREA / contenteditable → return（让用户在输入框里继续打字）
- preventDefault + setActiveTab

不动 KeyboardHelpOverlay 因为它只挂在 PanelApp（DebugApp 没有 `?` 帮助层）。

## 不做

- 不在 DebugApp 加 KeyboardHelpOverlay：该窗口本就是 debug 调试用，不需要"对终端用户解释快捷键"
- 不复用 PanelApp 的 keydown handler：跨窗口共享 React effect 需要抽 hook，scope 太大；inline 20 行单点实现更直观

## 验收

- `npx tsc --noEmit` ✅
- 调试窗口任一 tab 按 `⌘2` → 切到「日志」；`⌘3` → 「LLM 日志」；`⌘4` → 「统计」
- 在某 input 聚焦时按 `⌘2` → 不切（让出键位）

## 完成

- [x] DebugApp.tsx: 加 keydown effect
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
