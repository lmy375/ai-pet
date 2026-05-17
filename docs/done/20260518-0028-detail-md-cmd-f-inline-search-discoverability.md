# detail.md ⌘F 行内搜索（discoverability 补全）（iter #342）

## Background

TODO 项：「detail.md 编辑器 ⌘F 改为聚焦行内搜索（不抢 panel 顶搜索框）：
长 detail.md 文档内找文本快」。

审查代码发现该 capture-phase ⌘F 拦截 + 行内搜索 bar UI **已实现**于
更早某迭代：
- `useEffect` 注册 capture-phase keydown listener（`PanelTasks.tsx`
  L2267）：`window.addEventListener("keydown", onKey, { capture: true })`
- focus 在 detail textarea / detail search input 时拦下 ⌘F + 调
  `stopImmediatePropagation()` 阻止 useTaskKeyboardNav 的 bubble-phase
  ⌘F 抢顶搜索框
- 行内 search bar 渲染在 detail editor 区（L9425+）：input + match count
  + Enter / ↑↓ 切 match + Esc 关，textarea 通过 setSelectionRange 滚到
  命中点
- editingDetailTitle === null 时 listener 不挂；切 task 时清空 query

**但**：cheatsheet modal 没列这条快捷键 / placeholder 也没暴露 — owner
不知道 detail editor 里 ⌘F 是行内搜索而非"焦点顶部搜"。功能在，
discoverability 0。

本迭代补 discoverability — 让 owner 通过 ⌘/ cheatsheet 或 placeholder
hint 发现这个已存在的功能。

## Changes

仅 `src/components/panel/PanelTasks.tsx`：

- ⌘/ cheatsheet modal「detail.md 编辑器」段加新条：
  `["⌘F", "在 detail.md 内行内搜索（Enter / ↑↓ 切 match · Esc 关)"]`
  位置紧贴 ⌘⌥Enter 之后，与其它 ⌘ modifier 集群对齐
- textarea placeholder 文案补 `⌘F 行内搜本文`（两 textarea — edit +
  split mode — 都同步）

## Key design decisions

- **不重写已实现功能**：capture-phase intercept + stopImmediatePropagation
  + activeElement gate 已经精准覆盖所有用例（textarea / search input /
  其它 focus / detail closed），重写只会引入回归风险。
- **本 iter scope 仅 discoverability**：iter 报告诚实记录"feature 已存
  在，本次补 cheatsheet + placeholder 让 owner 发现"。比假装重写一遍
  更尊重既有 commit history。
- **placeholder vs cheatsheet 双发现路径**：
  - placeholder 是 owner 首次进编辑器立即看到的 hint
  - cheatsheet modal (⌘/) 是 owner 事后查询的索引
  两条路径冗余覆盖 — 任一条都能让 owner 学到新快捷键。
- **不动 listener 实现**：capture-phase + stopImmediatePropagation 是
  巧妙设计 — 避开了"useTaskKeyboardNav 的 ⌘F 在 tagName 守卫之前"这
  个跨 input 设计问题。再动会破坏整体平衡。
- **不引入 unit test**：纯 cheatsheet / placeholder 文案改；JSX 渲染
  无逻辑分支。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.25s)
