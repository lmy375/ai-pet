# PanelTasks 行右键「📋 复制 raw_description」（iter #310）

## Background

PanelTasks 行右键菜单已有「📋 复制 detail.md 全文」（仅 detail.md 内容）
和「📑 复制为 Markdown」（带元数据 bullet 头的完整段）。但缺一个"只
要 description 原始文本（含全部 markers）"的入口 —— owner 想 debug 一条
任务的 marker 组合 / 把 marker 组合移植到新任务 / 跨任务复用复杂 schedule
prefix 时，目前只能展开 task → 看到 textarea → 手抄 markers。

本迭代加「📋 复制 raw_description」ctx menu item，把 raw_description 一
键到剪贴板。

## Changes

仅 `src/components/panel/PanelTasks.tsx`：

- 行右键菜单加新按钮（在「📋 复制 detail.md 全文」之后）：
  - 仅 `t` 存在时渲染（与其它 task-action 按钮同 gate）
  - onClick：`navigator.clipboard.writeText(t.raw_description)` + 字符
    数 hint toast
  - 空 raw_description（极端情况）→ 反馈"没有内容可复制"，不真复制
  - tooltip 强调三个 ctx menu 复制路径的差异化定位：
    - 「📋 复制 raw_description」= 原始文本含 markers（本按钮）
    - 「📑 复制为 Markdown」= 带元数据 bullet 头的完整段
    - 「📋 复制 detail.md 全文」= 仅 detail 进度笔记

## Key design decisions

- **仅复制 raw_description（不附 title / detail）**：本按钮就是定位为
  "raw debug 视角" — 把 markers 完整组合给 owner 自己处理。owner 想加
  title / detail 走「📑 完整段」。
- **不 async fetch / 直接读 TaskView.raw_description**：raw_description
  已在内存（TaskView 字段），不需 task_get_detail 再去后端拉一次。这让
  本按钮零 IO 延迟。
- **空 raw 兜底文案**：butler_task 初始化时不会出现空 raw_description
  （memory_edit("create") 必带 description），但防御编程让边界行为可预
  期 —— 与 detail.md 全文按钮同 pattern。
- **复用既有 setBulkResultMsg 3s toast**：与既有 ctx menu 复制 action
  统一反馈渠道，owner 视觉 calibration 一致。
- **位置紧贴 detail.md 复制按钮**：三种复制路径相邻让 owner 看到差异化
  矩阵 —— 一眼能选对的入口。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.19s)
