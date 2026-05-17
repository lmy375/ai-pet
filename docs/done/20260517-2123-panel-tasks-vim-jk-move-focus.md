# PanelTasks J / K vim-style 移焦点快捷键（iter #331）

## Background

PanelTasks 键盘 nav 已有 ↑↓ 移焦点。vim / VS Code Vim plugin / 大量
keyboard-power-user 的 muscle memory 是 j/k 移光标 — 让 hand 不必离开 home
row。本迭代让 j/k 与 ↑↓ 同语义共存。

## Changes

仅 `src/components/panel/useTaskKeyboardNav.ts`：

- 既有 `if (e.key === "ArrowDown")` 检查扩展为 `|| isVimDown` — 加新
  guard：`e.key === "j"` + 无 metaKey / ctrlKey / altKey / shiftKey
- `ArrowUp` 同 `|| isVimUp` 加 `k` plain-key guard
- 守卫位置不变：tagName 守卫已挡 INPUT / TEXTAREA / SELECT / BUTTON
  焦点时不响应 — owner 在搜索 / 创建表单 / detail textarea 输入"j" /
  "k" 不被吞

PanelTasks ⌘/ cheatsheet modal:

- 任务列表段「↑ / ↓」行文案改 "↑ / ↓ 或 j / k" + 描述补"vim 风格"

## Key design decisions

- **plain key 无 modifier**：与既有 d / r / p / n 单键 plain-key 集群一
  致。⌘ / Shift / Alt 修饰一旦命中走其它语义（如 ⌘K 跳 task palette /
  ⌘D 复制 title）— 不冲突。
- **与 ↑↓ 完全 alias 而非新行为**：j/k 是"翻译层"— 不引入新交互范式（如
  hjkl 4 方向 / vim normal mode）。owner 不必学习新概念，只是多一组手
  指能用。
- **tagName 守卫自然 cover**：既有守卫已经 exclude 输入控件 — 不必额
  外为 j/k 加 guard。owner 在 INPUT 打 "j" 完全不被吞。
- **不引入 unit test**：与 d / r / p 同型 plain-key alias；既有 ↑↓ 也未
  单测；通过 vite build + 真实交互验证。
- **cheatsheet modal 文案合并**：「↑ / ↓ 或 j / k」一行 — 让 owner 看
  cheatsheet 时一眼知道两组同义。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.18s)
