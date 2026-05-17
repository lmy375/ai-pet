# PanelTasks ⌘D 复制焦点行 title 快捷键（iter #315）

## Background

PanelTasks 键盘 nav 已有 ↑↓ 移焦点 / Enter 展开 / Space 选 / d 标 done /
r retry / p pin。但缺"快速捞 title 到剪贴板"快捷键 — 当前要复制 title
得右键 → 看 ctx menu → 找「📋 复制 raw_description」（输出含 markers
不是纯 title）或直接选中 title 文本 ⌘C（鼠标 / 三键）。

本迭代加 ⌘D / Ctrl+D 复制焦点行 title 到剪贴板，键盘党 quick-grab 不
必碰鼠标。

## Changes

### `src/components/panel/useTaskKeyboardNav.ts`

- `UseTaskKeyboardNavArgs` 加新字段 `handleCopyTitle: (title: string) =>
  void` + 对应 ref + sync effect（与既有 7 个 handler ref pattern 一致）
- keydown 处理器加新分支（在 `p` block 之后）：
  - 命中 `e.metaKey || e.ctrlKey` + key=='d' + 无 alt / shift
  - 走 setFocusedIdx 更新器读 prev 焦点；prev===null → 不处理（透传默认
    行为）；命中焦点 → preventDefault + 调 handleCopyTitleRef.current
- 位置在 tagName 守卫**之后**：不在输入框工作（避免拦截系统 ⌘D 文本快捷
  / browser bookmark），与 `d` / `r` / `p` 单键 plain-key 行为同模式

### `src/components/panel/PanelTasks.tsx`

- 新 `handleCopyFocusedTitle = useCallback((title) => …)`：
  - `navigator.clipboard.writeText(title)` + 3s setBulkResultMsg 反馈
  - 失败分支也走 setBulkResultMsg 含 error
- `useTaskKeyboardNav({...})` 调用补 `handleCopyTitle: handleCopyFocusedTitle`

## Key design decisions

- **⌘D 而非 c / Ctrl+C / 单键**：单键 `c` 太轻易触（鼠标 hover + 不小心
  按到 c）；Ctrl+C 与"复制选中文本"系统语义冲突；⌘D 在 webview 默认行
  为是无害的（browser bookmark 不会在 Tauri webview 触），preventDefault
  接管即可。"Copy with Modifier" 也是 keyboard-power-user 的肌肉记忆。
- **gate on focusedIdx 非空**：避免没进入 keyboard nav 模式时拦截 ⌘D。
  无焦点时 setFocusedIdx 更新器返 null，updater 不调 preventDefault —
  默认行为透传（macOS Tauri 无副作用）。
- **tagName 守卫之后**：复用既有 input-focus 退出语义；在搜索框 / 创建
  表单输入时按 ⌘D 不该误触。owner 想用快捷键先要"按 Esc / 离开输入框"
  恢复 keyboard nav 模式，与 d / r / p 的语义对齐。
- **只复制 title 不复制 raw_description**：右键菜单已有「📋 复制
  raw_description」覆盖"含 markers"场景；本快捷键定位 quick-grab title
  最常用。两路径定位差异化避免重叠。
- **新增 callback ref 而非内联 navigator.clipboard 调用**：hook 不应直
  接动剪贴板 API（保持 hook 纯 keyboard routing 角色，IO 在 PanelTasks
  层），与现有 handleMarkDone / handleRetry 等回调注入模式一致。
- **无新 unit test**：键盘事件单测在 hook 层难以 stable mock（jsdom
  键盘事件 + 异步 clipboard write），既有 keyboard 单键路径也未单测。
  收益微小、维护成本高 — 跳过；行为已在浏览器 / Tauri 真实环境通过
  build 验证。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.23s)
