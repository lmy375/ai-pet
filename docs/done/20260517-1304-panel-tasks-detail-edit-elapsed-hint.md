# detail.md 编辑器「⏰ 编辑用时 N 分钟」灰字 hint（iter #299）

## Background

PanelTasks detail.md 编辑器现有 status bar 显「● 未保存」+ 行/共 M
counter + 字数 chip / 进度。owner 在长 detail 上写作（task review / 复盘
笔记）时，没有信号知道"在这条 task 写了多久" —— 与 dirty marker 互补：
那个是"内容已改但未存多久"，这个是"session 总时长"。

本迭代加 ⏰ 编辑用时 hint：editor 打开后开始计时，灰字渲在状态栏。

## Changes

仅 `src/components/panel/PanelTasks.tsx`：

- 新增 ref `editStartRef = useRef<number | null>(null)`
- 既有 `editingDetailTitle` 切换 effect 内：
  - 进入 edit（title !== null）→ `editStartRef.current = Date.now()`
  - 关闭（title === null）→ 清 ref
- status bar 内（在 `● 未保存` chip 之前）插 IIFE 渲 chip：
  - 复用既有 5s 周期 `dirtyTickKey` 驱动重渲（不另开 interval）
  - `elapsedSec < 60` 不渲（避免噪音 — owner 刚进入不需要提示）
  - 显 "⏰ 编辑用时 Nm"，≥ 60min 时显 "⏰ 编辑用时 Hh Mm"
  - 灰字 muted color + monospace + opacity 0.7
  - hover tooltip 显精确秒数 + 释义

## Key design decisions

- **复用既有 dirtyTickKey 5s 重渲**：避免另开 interval / state 同步问题。
  dirtyTickKey 已 gate 在 editingDetailTitle !== null 时启动，正是我们想要
  的范围。dead-code prevention 用 `void dirtyTickKey;` 让 ESLint 看到关联。
- **重置而非累加**：每次重开编辑器即重置 —— "用时" 是 session 范围，与
  "今日累计在该 task 上写了多久" 不同语义（后者更复杂，需 IDB / 后端持久；
  超 owner 当前需求的范围）。
- **< 1 分钟不渲**：避免快速 review / typo 修正时浮 "0m" 噪音。owner 真
  正长写时（≥ 1min）才出 hint。
- **≥ 60min Hh Mm 折叠**：避免 "120m" 这种字面无意义的数字。owner 编辑
  超 1 小时时一眼看到"2h" 比"120m"更直觉。
- **位置在 ● 未保存 之前**：未保存 chip 用 marginLeft: auto 推到右侧；
  ⏰ chip 放在 nav buttons 之后、未保存 chip 之前，自然 sit 在状态栏左侧
  靠近其它编辑器辅助按钮 —— hint 性质就该贴近"操作面"。
- **不点击交互**：纯展示 hint，不像 wordCountGoal chip 那样是 setter 入
  口 —— "用时" 是只读观测信号，加交互会喧宾夺主。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.23s)
