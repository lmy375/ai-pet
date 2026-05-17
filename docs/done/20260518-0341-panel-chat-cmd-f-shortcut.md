# PanelChat ⌘F 行内搜索快捷键（iter #354）

## Background

PanelChat 已有跨 session 搜索 UI（searchMode / searchQuery / searchScope
state + searchBar render with autoFocus），但当前只能通过：
- 顶部「🔍」按钮 click 触发
- 「🔍 再细说」消息上下文菜单触发

owner 在 chat 长会话里想找历史消息时没标准 ⌘F 入口 — 与浏览器 / IDE
/ Finder 的"⌘F = 在当前视图找"直觉冲突。

本迭代加全局 ⌘F 监听 → setSearchMode + setScope("current") → 渲染 search
bar，autoFocus 自动把焦点拉到 search input。

## Changes

仅 `src/components/panel/PanelChat.tsx`：

- 新 useEffect 注册 window keydown listener：
  - 命中 `(metaKey || ctrlKey)` + key=='f' + 无 shift / alt
  - preventDefault 吃浏览器默认 "find in page"
  - setSearchScope("current") — ⌘F 直觉是"在这里找"，对应 current session
  - setSearchMode(true) — 触发既有 search bar 渲染 + autoFocus
- 位置：紧贴既有 ⌘K / ⌘N 全局 useEffect（同 pattern — window keydown
  + cleanup），让全局 hotkey 集群相邻

## Key design decisions

- **跨 input context 工作（不让位 textarea / input）**：与既有 ⌘K
  / ⌘N 全局监听 bail-on-input-focus 不同 — ⌘F 是 view-level 动作
  ("查找历史")，owner 写到一半按 ⌘F 是明确"先搜后说"意图，应抢焦点。
  与 PanelTasks / PanelMemory 的 ⌘F 行为对齐（也是跨 input 工作）。
- **默认 scope=current**：⌘F 直觉是"在这里找"（与浏览器 cmd+F 同 ——
  搜本页）。跨 session 想法走 search bar 内的 scope chip 切换。
- **复用既有 searchMode / scope / autoFocus 机制**：不引新 state /
  新 input — 仅添 keyboard binding 作 affordance。既有 search bar 的
  Enter 触发查询 / Esc 关 / scope chip 切换 / 结果列表全套已 wired。
- **不引入 unit test**：global keydown listener + 既有 setSearchMode
  trigger；jsdom keyboard event mock 维护成本与既有 ⌘K / ⌘N 同处境，
  通过 vite build + 真实交互验证。
- **不更新 cheatsheet**：PanelChat 没有专属 cheatsheet modal（PanelTasks
  的 ⌘/ cheatsheet 是 task-panel-scoped）。owner 在 chat 内按 ⌘F 是肌
  肉记忆行为 — 浏览器 / 任何文本视图都是这样，不必教育。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.25s)
