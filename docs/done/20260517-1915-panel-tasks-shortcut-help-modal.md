# PanelTasks ⌘/ 快捷键速查 modal（iter #324）

## Background

PanelTasks 累积了 16+ 键盘快捷键（↑↓ / Home/End / Enter / Space /
Delete / d / r / p / n / `/` / ⌘F / ⌘K / ⌘N / ⌘D / ⌘R / ⌘[ ⌘] / detail
editor 内 ⌘S / ⌘⇧Enter / ⌘D / ⌘L 等）。但快捷键散落在各 handler 注释 +
placeholder 文案里，新 owner 看不到完整 map，老 owner 也会忘 "那个复制
focused title 是 ⌘D 还是 ⌘C"。

本迭代加 ⌘/ 弹快捷键速查 modal — 列全 PanelTasks 范围内所有快捷键 +
用途，按 "全局 / 任务列表 / 创建表单 / detail editor" 四段分组。

## Changes

### `src/components/panel/useTaskKeyboardNav.ts`

- 加新 arg field `handleShowShortcutHelp: () => void` + ref + sync effect
- keydown 处理器最前段（与 ⌘F / ⌘K / ⌘R 同跨 input 工作）插 ⌘/ / Ctrl+/
  分支：preventDefault + 调 handler ref

### `src/components/panel/PanelTasks.tsx`

- 新 state `shortcutHelpOpen: boolean` + `handleShowShortcutHelp` toggle
  callback（再按 ⌘/ 关）
- useTaskKeyboardNav 调用补 handleShowShortcutHelp
- 在 panel 最后插 modal 渲染：
  - fixed overlay + dark backdrop（rgba .45）+ zIndex 9999
  - 内容卡居中，maxWidth 580 / maxHeight 80vh / overflowY auto 长溢出
    可滚
  - 头部「⌨️ PanelTasks 快捷键速查」+ 右侧「Esc 关」mini button
  - 四段 grouped list：
    1. 🌐 全局（跨 input 工作）— ⌘F/K/`/` 聚焦搜索 / ⌘R 立即刷新 / ⌘/
       本 modal
    2. 📋 任务列表（focused row）— ↑↓ / Home/End / Space / Enter /
       Del / d / r / p / ⌘D
    3. 🆕 创建表单 — n / ⌘N / ⌘⇧Enter
    4. 📝 detail.md 编辑器 — ⌘S / ⌘⇧Enter / ⌘D / ⌘L / ⌘[ ⌘] / Esc /
       Enter list-continue
  - 每行 `<kbd>` 按键 + 描述 grid 两列（auto / 1fr）对齐
  - 底脚 dashed-top divider 显"点击空白 / Esc / 按钮 / 再按 ⌘/ 均可关"
- backdrop onClick → 关；modal 内容 onClick stopPropagation 防误关；
  onKeyDown Esc 拦截关；autoFocus 让 Esc 立即生效

## Key design decisions

- **跨 input context 工作（与 ⌘F / ⌘K / ⌘R 同放最前）**：cheatsheet 是
  通用 affordance — owner 在搜索框 / 创建表单 / detail textarea 都可能
  忘快捷键。如果只在 tagName 守卫之后响应，正在输入时按 ⌘/ 没反应反而
  添堵。
- **`⌘/` 再按 toggle 关**：与 macOS 系统级 "再按相同快捷键关" 习惯一致
  （如 Spotlight 再按 ⌘Space 关）。state 用 `(v) => !v` 不强 set
  true，避免按一次后状态卡死。
- **inline shortcut 列表 in source code（非外部 const / JSON）**：可读
  性优先 — 维护时改文案就在原地，不必跨文件 sync。修改快捷键时也容易
  发现 modal 文案过时。
- **section grouping 而非 alphabetical**：按"何时按 / 焦点在哪"分组让
  owner 心智匹配 — 在搜索框时关注 🌐 段，在列表 nav 时关注 📋 段。字
  典序在长 list 反而难找到目标。
- **不引入 react-portal**：modal 在 panel root 内 fixed 定位 + zIndex
  9999 已足够穿透其它 sticky 元素；省一个 createPortal import。
- **Fragment 内 grid 两列布局**：每条快捷键是 `<Fragment><kbd>...</kbd><span>...</span></Fragment>`，grid 自动分两列均匀 — kbd 列 auto 宽度，
  描述列 1fr 撑满。比 `<div>` row 用 flex 更紧凑（无 row padding 累积）。
- **不引入 unit test**：纯 modal JSX；键盘事件单测在 jsdom 难稳；通过
  vite build + 真实交互验证。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.23s)
