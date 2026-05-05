# 任务面板键盘选中 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> 任务面板键盘选中：长队列里全靠鼠标点 checkbox，加方向键 + 空格切换选中支持，键盘党也能批量。

## 目标

「任务」面板长队列下逐条点 checkbox 累。本轮加键盘导航：
- ↑ / ↓ 在 `visibleTasks` 中移动"焦点"索引（蓝边视觉提示 + 自动滚动到视区）
- 空格 切换焦点任务的选中状态（与点 checkbox 等价）
- 焦点在输入框 / textarea / select 上时**不**截获（让原生输入正常）
- 焦点超出 visibleTasks 范围时自动 clamp（批量操作后任务消失也不报错）

## 非目标

- 不做 Shift+方向键的"扩展选区" —— 多选已通过空格逐条 toggle 实现，扩展选
  区在键盘党之外用得少。
- 不做 Home / End 键跳首尾 —— 长按 ↑↓ 已能滚到底；4 个键覆盖最频。
- 不做 Enter 键展开详情 —— 详情 accordion 是鼠标模型，键盘版会让 Enter
  与 form submit 冲突（我们没有 form，但保留鼠标语义清晰）。
- 不写 README —— 任务面板键盘可达性补强。

## 设计

### 状态

`focusedIdx: number | null` —— null 表示尚未启用键盘导航（首次按 ↓ 才进入
focus 模式）。这样默认行为与现状一致：鼠标用户看不到任何视觉变化。

### 键盘监听

mount 时 `window.addEventListener("keydown", handler)`。handler：
1. 若 `e.target.tagName ∈ {INPUT, TEXTAREA, SELECT}` → 直接 return（不截获原生
   输入；用户在 search / 创建任务表单里打字时方向键不应跳 row）
2. 若 `e.key === "ArrowDown"`：preventDefault；focusedIdx = `(prev ?? -1) + 1`，
   clamp 到 `visibleTasks.length - 1`
3. 若 `e.key === "ArrowUp"`：preventDefault；focusedIdx = `max(0, (prev ?? 0) - 1)`
4. 若 `e.key === " "`（空格）：preventDefault；若 `focusedIdx !== null` 且
   `visibleTasks[focusedIdx]` 存在 → 调 `toggleSelect(title)`

监听挂在 window 而非局部 div 的原因：用户可能没把焦点显式落到 panel（默认初
始 focus 在 body）；window 监听 + tagName 守卫足够过滤。

### 视觉

每行已有 `data-task-idx={i}`（任务详情 expand 用）—— 复用。focused row 加 inline
`outline: 2px solid #93c5fd; outline-offset: -2px` 蓝色光环（不挤占布局；与
搜索高亮 / 任务展开背景同色族但不互冲）。

`useEffect` on `[focusedIdx]` 调 `querySelector` + `scrollIntoView({ block:
"nearest", behavior: "smooth" })` 让聚焦行进可视区。

### 数据一致性

`useEffect` on `[visibleTasks.length]`：若 `focusedIdx !== null && focusedIdx
>= length`，clamp 到 `length - 1`（或 `length === 0` 时设 null）。这样批量
操作 / search / showFinished 后焦点不悬空。

### 测试

逻辑全在前端 React effect / event handler 里，项目无 vitest。靠 tsc + 手测。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | focusedIdx state + window keydown listener + clamp effect |
| **M2** | 行视觉 outline + scrollIntoView |
| **M3** | tsc + build + cleanup |

## 复用清单

- 既有 `data-task-idx={i}` 行属性（已为详情 expand 用）—— 这次直接复用
- `toggleSelect(title)` / `visibleTasks` / `s.item` 既有派生

## 待用户裁定的开放问题

- 用户在 search 中按 Down 期待跳第一行？本轮**否** —— 截获 input 内的方向
  键会破坏文本光标移动；如反馈想要可特判 `INPUT[type=text]` + 是 search
  框时跳出 + focus 转移到 panel。
- 焦点视觉用 outline vs background？outline —— 不影响行内布局，与"搜索高亮"
  的背景色错开（搜索 = 黄背景；焦点 = 蓝边）。

## 进度日志

- 2026-05-05 22:00 — 创建本文档；准备 M1。
- 2026-05-05 22:30 — 完成实现：
  - **M1**：`PanelTasks.tsx` 加 `focusedIdx: number | null` 状态（null = 尚未启用键盘导航，与鼠标用户体验一致）；window keydown 监听通过 ref-pattern（`visibleTasksRef` / `toggleSelectRef`）只挂载一次，读最新状态。tagName 守卫（INPUT / TEXTAREA / SELECT）让用户在搜索 / 创建表单 / 取消原因输入里打字时方向键 / 空格不会跳行。
  - **M2**：行 outer div 加 `data-task-idx={idx}` + 焦点态 inline outline（`2px solid #93c5fd, offset -2px`，与搜索黄背景错开）；clamp effect on `[visibleTasks.length]` 防越界；scroll effect on `[focusedIdx]` 让长队列翻页跟随视图。
  - **M3**：调整代码顺序——keyboard nav block 必须放在 `visibleTasks` / `toggleSelect` 之后（TS use-before-declaration 错误修复后位置稳定）；`pnpm tsc --noEmit` 干净；`pnpm build` 497 modules 全过；`cargo test --lib` 885/885。TODO 移除条目；本文件移入 `docs/done/`。
  - **README 不更新** —— 任务面板键盘可达性补强。
  - **设计取舍**：window 监听 + tagName 守卫 vs 局部 div onKeyDown（需先点击聚焦）—— 选前者，键盘党无需先点 panel 任意位置；ref-pattern 让 effect 只挂一次，避免每次 visibleTasks 变化都 re-subscribe 的窗口竞态；focused === null 默认（鼠标用户无视觉变化），↑↓ 启动焦点模式后才看到蓝边。
  - **未做手动 dev 验证**：当前会话不便启动 Tauri 桌面 app；逻辑由 tsc + ref-pattern（与 App.tsx 既有模式同源）保证。
  - **TODO 后续**：列表清空后按规则提 5 条新候选（任务面板回车展开 / 桌面气泡多行 markdown / 设置面板搜索高亮 / 任务行最近更新指示 / TG 心跳静默通知）。
