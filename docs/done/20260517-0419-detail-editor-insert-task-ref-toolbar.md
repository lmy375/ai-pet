# detail.md 编辑器 toolbar「」插 task ref 按钮（iter #247）

## Background

owner 在写 detail.md 进度笔记时常想 reference 另一个 task — 比如「这个分支
要等 `「DB 迁移」` 完成才能合」。已有协议：`「title」`（全角直角引号）是
ref token，renderContentWithTaskRefs 渲染时是 hover 显状态 / 双击跳源任务的
chip。

此前要 ref：要么手敲 `「title」` + 自己确认拼写，要么 PanelTasks 多选 + bulk
"🔗 拼为 ref" 写剪贴板 + 切回 detail 粘贴。两条路都打断 detail 编辑节奏。

本迭代在 detail 编辑器 markdown toolbar 加「」按钮 → 复用现有 ⌘K palette
基础设施（iter #240）但跑 insertRef mode → 弹 fuzzy picker → 选中即在光标位
置插 token。

## Changes

仅 `src/components/panel/PanelTasks.tsx`：

- **state 扩展**：在既有 `taskPaletteOpen` 旁加 `taskPaletteMode: "jump" |
  "insertRef"`（默认 `"jump"`）。

- **`insertTaskRefAtCursor(title)` callback**：从 `detailEditorRef.current`
  拿 `selectionStart / selectionEnd`，在 `editingDetailContent` 上替换选区
  为 `「title」`，rAF 后 refocus + setSelectionRange 把光标落 token 末尾。

- **⌘K 路径继续是 jump mode**：⌘K handler 显式 `setTaskPaletteMode("jump")`，
  防止上次 insertRef 调起后 mode 残留。

- **palette Enter / click 分流**：根据 `taskPaletteMode` 走
  `insertTaskRefAtCursor` 或 `switchToTaskDetail`。disabled 规则 jump 模式下
  禁用 isCurrent，insertRef 允许自引（自引可能少见但合法，比如往主任务
  detail 写「子任务 = `「sub-X」`」时主任务 = sub-X 父任务的 ref）。

- **placeholder + title 切换**：mode 切到 insertRef 时输入框 placeholder /
  按钮 tooltip 都改成「插 ref token」语义，让 owner 知道当前在哪个模式。

- **toolbar 按钮**：在 detail editor markdown toolbar 的 `✓` 完成行按钮之后
  插入「」按钮。click → 开 palette + 设 mode + 清 query。

## Key design decisions

- **复用 palette UI 而不是新 picker**：⌘K palette 已有完整 fuzzy 过滤 / ↑↓
  键盘选择 / Esc 关 / mouse hover sync 等交互，重写一遍只为 insertRef 是巨大
  重复。加一个 mode flag 让同 UI 双用 — 状态管理略复杂但 UI 一致性高。
- **插 token 用「」全角直角引号**：与 renderContentWithTaskRefs / bulk
  「🔗 拼为 ref」协议同形 — 渲染时是 hover-able chip，否则需要另立协议。
- **rAF 后 refocus + setSelectionRange**：插 token 后立即 focus 会因 React
  rerender 把 selection 还原到 setState 之前的位置；rAF 等下一帧 textarea
  value 已是新值，setSelectionRange 才落在正确位置。
- **按钮标签直接用「」二字**：emoji icon 库里没有专表「ref / link」且不与
  既有 🔗 (markdown URL link) 冲突的图标；用 token 本身作图标自文档。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.19s)

## Notes

owner 还可以继续走 ⌘K palette（jump mode）切到目标 task — 两条路径
（jump / insertRef）由触发入口决定，互不干扰。
