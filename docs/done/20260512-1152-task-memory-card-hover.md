# Task / Memory 卡片 hover 抛光（UI 美化 迭代 4）

## 背景

PanelTasks 任务卡 / PanelMemory 记忆条目原 hover 仅切 `background-color`（var(--pet-color-bg)）。在长列表里"我现在 hover 在哪里"的视觉锁定还不够强 —— 仅 bg 反差一档容易在浅色主题里看不出。

## 改动

`PanelTasks.tsx` 内 `.pet-task-card:hover` + `PanelMemory.tsx` 内 `.pet-memory-item:hover`：

- 加 `box-shadow: var(--pet-shadow-sm)` 制造"卡片轻浮起"感（复用迭代 1 全局 token）。
- 加 `border-color: color-mix(<accent> 35%, <border>)`，hover 时 border 偏 accent，明确"我在指向这条"。
- transition 扩到 `background-color + box-shadow + border-color`（PanelTasks 也加 `transform`，留给后续 micro-translate 用，目前不动）。
- 跨两个面板用同一组规则，hover 节奏一致。

## 不做

- 不加 `transform: translateY` 微浮 —— task / memory 行底部紧邻下一行，translate 易触发垂直闪烁。
- 不动 row 内子元素（badge / detail.md / due chip 等）。
- 不写测试 —— 纯 CSS hover，无可 pin 行为。

## 验收

- 在任务列表 / 记忆列表上鼠标移动，被 hover 的卡片 bg 略变 + 边框偏 accent + 浮起一道淡阴影；离开顺滑回落。
- 浅 / 深主题下 `--pet-shadow-sm` 自动跟随（迭代 1 已分主题定义）。
- `npx tsc --noEmit` 通过。

## 完成

- [x] PanelTasks.pet-task-card hover 规则更新
- [x] PanelMemory.pet-memory-item hover 规则更新
- [x] 移到 docs/done/
