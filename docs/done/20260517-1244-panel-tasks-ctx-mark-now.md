# PanelTasks task ctx menu「⚡ mark NOW (60s)」命令（iter #293）

## Background

`⚡ NOW` marker（60s 内浮顶 + 桌面 nudge）当前只能通过 detail.md 编辑器
头部的 ⚡ chip / panel 头部某些 chip 触发。右键 ctx menu 上没入口 — owner
看到 task row 想立刻标 NOW 时要先展开 detail 才行。

本迭代在右键 ctx 菜单的「📂 展开详情」之后插「⚡ mark NOW (60s)」命令，
调既有 `markTaskNow(title)` helper：60s 内浮顶 + emit task-now-mark 事件
让桌面 pet 收 nudge。

## Changes

仅 `src/components/panel/PanelTasks.tsx`：

- 右键 ctx 菜单的「📂 展开详情」之后插 ⚡ 按钮：
  - 仅 `t && !nowMarkedTitles.has(m.title)` 时显（已 NOW marked 时不重复
    显，避免"再点 reset 60s"歧义；想 reset 走 detail header ⚡ chip）
  - click → `setTaskCtxMenu(null)` + `markTaskNow(m.title)`
  - orange tint color 与既有 ⚡ NOW chip 色感一致

## Key design decisions

- **已 marked 时按钮隐藏而非禁用 / "reset" 语义**：disabled 按钮在 ctx
  菜单里浪费一行视觉位；reset 60s 是更高级用法，留给 detail header 的
  ⚡ chip。本菜单按钮覆盖"我没标过，现在想标"主用例。
- **复用 markTaskNow helper**：既有 helper 已包 timer / event emit / map
  state — ctx 菜单只需在 closure 内调一次即可继承全部副作用（含桌面
  nudge）。
- **位置紧贴「📂 展开详情」**：两条都是"我注意到这条 task"的快速反应路径
  — 一个深入看（展开），一个表达紧迫（NOW）。视觉相邻让 owner 在两条间
  快速决定。
- **orange tint color**：与既有 nowMarked task row 浮顶 chip 色系一致，让
  "标 NOW" 动作和"已 NOW"状态色感连贯。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.19s)
