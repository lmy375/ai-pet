# PanelTasks ⌘E 展开 / 折叠 focused row 快捷键（iter #334）

## Background

PanelTasks 键盘 nav 已有 Enter 切换焦点行展开 / 折叠。但 Enter 容易让人
联想到"提交 / commit"语义；某些 owner 习惯用更显式 modifier 表达"展开
详情"动作（如 VS Code ⌘B toggle sidebar / Finder ⌘↓ open）。

本迭代加 ⌘E / Ctrl+E 作为 Enter 的 alias — `E = Expand` 助记好；与既有
⌘D 复制 title / ⌘R 刷新 modifier-cluster 一致。

## Changes

仅 `src/components/panel/useTaskKeyboardNav.ts`：

- keydown 处理器末段加新分支（在 ⌘D 之后）：
  - 命中 `(e.metaKey || e.ctrlKey)` + key=='e' + 无 alt / shift
  - focusedIdx 非空时拦截 → preventDefault → 调
    `handleToggleExpandRef.current(item.title)`（既有 Enter handler 同
    ref）
  - 无焦点时 setFocusedIdx 更新器返 null（透传默认 — macOS ⌘E "Use
    Selection for Find" 在 webview 内通常无害）

PanelTasks ⌘/ cheatsheet modal:

- 任务列表段「Enter」行文案改 "Enter 或 ⌘E" 让 owner 看 cheatsheet 时
  知道两组同义。

## Key design decisions

- **复用 handleToggleExpandRef**：与 Enter 同入口确保两路径行为完全一
  致（fire-and-forget toggle）。owner 切换 ⌘E / Enter 体验无差异。
- **modifier + 单字母 vs plain key**：Enter 是 plain key，⌘E 是修饰键 —
  与 d / r / p 单键 vs ⌘D 复制 title 同分层。owner 心智 "明确意图 ↔
  modifier 修饰"。
- **placement 在 ⌘D 之后**：与既有 ⌘D row-context shortcut 相邻让
  modifier cluster 紧凑；都在 tagName 守卫**之后** = 不在输入框工作。
- **focusedIdx 非空 gate**：与 ⌘D 同 pattern — 无焦点时透传，避免 macOS
  ⌘E 系统语义被无脑接管。
- **不引入 unit test**：与既有 ⌘D / ⌘R / J/K alias 同型 plain-modifier
  shortcut，键盘事件单测在 jsdom 难稳；通过 vite build + 真实交互验证。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.20s)
