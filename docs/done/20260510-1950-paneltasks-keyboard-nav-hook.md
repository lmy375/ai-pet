# PanelTasks 键盘导航 hook 抽取

> 对应需求（来自 docs/TODO.md）：
> PanelTasks 抽出键盘导航 hook（约 100 行），把 useEffect / refs / state 移到 useTaskKeyboardNav，主组件压缩到方便审阅的体量。

## 改动

新增 `src/components/panel/useTaskKeyboardNav.ts`：

- 接受 `visibleTasks / toggleSelect / handleToggleExpand /
  handleCancelOpen / searchInputRef / titleInputRef /
  setCreateFormExpanded / setFocusedIdx`。
- 内部封装 4 个 ref-update useEffect、1 个 keydown 全局监听 + cleanup、
  1 个 visibleTasks.length clamp useEffect。
- 业务态完全留给调用方（`focusedIdx` 等 state 仍由 `PanelTasks` 持有），
  hook 自身不持有任何业务 state。

`PanelTasks.tsx`：

- 主体从 3055 行压到 2919 行（-136 行 inline keydown）。
- 调用 `useTaskKeyboardNav({...})` 一行替换原来 145 行的 ref / effect /
  keydown handler 块。
- 行为不变：⌘F、`/`、`n`、↑↓、Home / End、空格、Enter、Delete / Backspace
  全部走原逻辑。

## 不变量

- 监听器仍只挂一次 —— hook 内部用 ref 持最新依赖。
- `clamp` useEffect 仍只在 `visibleTasks.length` 变化时跑。
- 无新增 IPC / 后端依赖。

## 验证

- `tsc --noEmit` 干净。
- `vite build` 干净。
