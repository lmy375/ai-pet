# PanelTasks 拖拽改 priority

## 需求

改 task priority 当前要点 P badge → 弹 P0..P9 picker → 选 → 关闭，3 步。
若用户想"把这条搞得跟那条一样重要 / 一样不急"，拖来拖去更直接。

实现"拖卡片到另一条上 → 自己 priority 改成对方的 P 值"。

## 实现

### 范围

- 仅在 `sortMode === "priority"` 启用 draggable。其它 sort 下（queue / due）
  拖卡片"位置 → P 值"映射不直观（按 due 拖会让卡片自己跳走，反馈错乱）。
- 不做"插入位置 → 计算连续 P 值"：P0..P9 是离散十档没有 in-between，落
  到哪条头就用那条的 P 是最直白的"我要和它一样重"语义。

### `src/components/panel/PanelTasks.tsx`

- 新 state：
  - `dragSourceTitle: string | null`（被拖的 task title）
  - `dragOverTitle: string | null`（当前 dragOver 目标 title，给目标卡 outline 用）
- 新 `handleDragDropPriority(source, target)`：
  - source === target / target 不存在 → return
  - target.priority === source.priority → 静默（避免无 invoke 的 reload 闪烁）
  - 调 `task_set_priority(source, target.priority)` + reload
  - 失败写 actionErr
- task card div 加 5 个 DnD handler：
  - `draggable={sortMode === "priority"}`
  - `onDragStart` → 写 source + `dataTransfer.setData("text/plain", title)`
    （部分 WKWebView 要求非空 payload）
  - `onDragEnd` → 清 source + over
  - `onDragOver` → 仅当 source 存在且不是自己时 preventDefault + 写 over
  - `onDragLeave` → 清 over（仅匹配的）
  - `onDrop` → 调 handleDragDropPriority
- 视觉：
  - dragEnabled 时 `cursor: grab`
  - isDragSource（自己被拖） → `opacity: 0.4`
  - isDragOverTarget（hover 落点） → 蓝虚线 outline + `--pet-tint-blue-bg` 浅蓝底
- sort mode 切换条 priority 按钮旁加 "· 可拖" 小字 + tooltip 提示用法（仅
  priority 模式时显，避免 queue / due 模式下 dead hint）

### 边角

- 终态 task（done / cancelled）也允许被拖：P 改对它们仅影响排序展示
  （retry 后回 pending 时排队仍参考 P）
- 与既有 `onClick` (展开) / `onContextMenu` (右键菜单) / checkbox 互不干扰：
  - HTML5 draggable + onClick：浏览器在判定 click vs drag 时按位移阈值
    分流，单击仍触发 expand；轻拖才进 dragStart
  - checkbox 自己有 `onClick stopPropagation` 阻冒泡

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - sort 切到 P↓：每个任务卡 cursor 变 grab；按钮组旁出现"· 可拖"
  - 拖一条 P3 任务到 P0 任务卡上 → 蓝虚线 outline + 浅蓝底 → 松手 → P3 变 P0
  - 拖到同 P 的卡：noop（无 reload 闪烁）
  - 拖到自己：noop
  - 拖出 panel 外松手：onDragEnd 收尾，无 invoke
  - sort 切回 queue / due：cursor 回 default，drag 不再响应

## 不在本轮范围

- 没做"拖到 P badge picker 浮层中间槽"那种"显式落到 P5"的精确控制 —
  右键菜单 + 现有 picker 已覆盖；拖拽是"跟另一条对齐"快捷
- 没做触屏 / touch drag：HTML5 DnD 在 macOS Tauri WKWebView 默认就是
  鼠标，触屏面板暂不在用户场景里
- 没做"拖出列表丢弃 / 改 status"：危险动作不走拖；右键 / button 入口够清

## TODO 池新提案（5 条，按规则 #1 自动补）

1. PanelChat compose 区 📎 文件选择器（vision 模型用）
2. PanelSettings 主题色自定义 accent
3. PanelTasks 完成任务统计小卡（今日 / 本周已完成 N）
4. PanelMemory 一键导出 .md zip
5. ChatMini ⌘F inline 搜历史消息
