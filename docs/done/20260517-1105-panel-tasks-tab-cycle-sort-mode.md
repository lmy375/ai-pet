# PanelTasks 顶部 sortMode 加 Tab 键循环切换（iter #284）

## Background

PanelTasks 顶部 sortMode 已有 4 个按钮：队列 / due ↑ / P ↓ / 📊 tag。owner
要切换 sortMode 必须鼠标点 — 键盘党 / IDE 习惯派的 owner 想要"按一下键
就循环"的入口。

本迭代加 Tab 键循环切换：focus 不在输入控件时按 Tab → queue → due →
priority → tag → queue 顺序循环。修饰键 / 输入控件 focus 时让位给系统
默认行为。

## Changes

仅 `src/components/panel/PanelTasks.tsx`：

- 新增 useEffect 挂 `window.keydown` 监听：
  - 仅响应 plain Tab（无 ⌘/Ctrl/⌥/⇧ 修饰）
  - tagName 守卫：INPUT / TEXTAREA / SELECT / BUTTON / contentEditable
    跳过，让原生 Tab 焦点跳转仍在表单内有效
  - 命中时 `e.preventDefault()` 吃掉浏览器 / Tauri webview 默认 Tab 行为
  - `setSortMode((cur) => order[(order.indexOf(cur) + 1) % 4])` 循环
- sortMode 按钮容器 tooltip 末尾加"焦点不在输入框时按 Tab 循环切换"提示

## Key design decisions

- **Tab 而非 `T` / 数字键 / 修饰组合**：Tab 没有歧义（normally 仅焦点跳转，
  在表单外几乎不用），plain Tab 可用作 sortMode 切换。`T` 可能冲突未来
  task-create 等单字快捷；数字键浪费在 4 个 mode 上。
- **tagName 守卫**：与既有 useTaskKeyboardNav 的"在 input 内不抢键"模板
  一致。owner 在搜索 / 创建表单里 Tab 应该跳焦点而非切排序。
- **修饰键全部跳过**：⇧Tab 反向焦点跳 / ⌥Tab 是窗口切换 / ⌘Tab 应用切换
  都该让位给系统。
- **不替代鼠标点按钮**：toggle 按钮组保留 — Tab 是辅助加成，不是唯一入口。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.20s)
