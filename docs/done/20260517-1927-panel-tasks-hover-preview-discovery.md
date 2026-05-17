# PanelTasks 任务行 hover preview discoverability 提升（iter #376）

## Background

TODO 写"PanelTasks 任务标题 hover 0.5s 后浮 detail.md 前 200 字 preview
tooltip"，实际查代码发现 hover preview 完整实现已存在（PanelTasks.tsx
~1407 `taskPreviewHoverTitle` 状态 + ~8121 `onMouseEnter` + ~8170
JSX 渲染）：
- 500ms hover delay ✓
- 600 char detail snippet（比 TODO 说的 200 还多）
- 含 chips（priority / due / tags / detail size）+ 最近 3 条 history
  + detail.md snippet 三段式
- 复用 detailMap 缓存（与 expand 视图同源）

iter #367 / #376 同 pattern — 功能已在但 owner 没发现 / 没用上，
说明缺 discoverability hint。

Pivot：title attribute 加 "💡 鼠标停留 0.5s 浮 ..." 一行 hint，让
owner 第一次接触行的 OS tooltip 时即看到 "row 有 0.5s hover 浮高
级 preview" 信号。

## Changes

### `src/components/panel/PanelTasks.tsx`（line 8504）

#### Before

```tsx
title={
  `${expanded ? "点击折叠详情" : "..."}\n\n原始 description：\n${...}`
}
```

#### After

```tsx
title={
  `${expanded ? "点击折叠详情" : "..."}\n💡 鼠标停留 0.5s 浮 detail.md 进度笔记 + chips + 最近历史 preview\n\n原始 description：\n${...}`
}
```

新增一行 hint 在 expanded/folded 提示之后、原始 description 段之前。
不动 onMouseEnter / state / 渲染逻辑（已正常工作）。

## Key design decisions

- **不改 hover delay (500ms) / snippet length (600)**：现有数值经
  实际使用磨合过；TODO 写的 200 是 author 估计值，不该硬塞回更
  小数（600 让 owner skim 更多上下文价值高）。
- **不缩窄 trigger 到仅 title span**：TODO 字面要"任务标题 hover"
  但实际 trigger 在整 row。缩窄会让光标移到 row 内其它位置时
  preview 闪烁消失（坏 UX）。row hover + delay 是更好的 affordance。
- **discoverability hint 用 💡 emoji**：与既有 cheatsheet / 错误提
  示文案风格一致；hint 与 OS tooltip 同源，hover 行就出现，不需
  额外 UI。
- **不引入 keyboard shortcut 触发 preview**：scope creep — 现有
  hover-on-row 已是合理 UX。如未来需要"无鼠标也 preview"再加 ⌘?
  之类。

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.24s)
- 后端无改动
