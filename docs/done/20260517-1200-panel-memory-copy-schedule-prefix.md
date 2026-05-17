# PanelMemory butler_tasks「⏰ 复制 schedule prefix」按钮（iter #289）

## Background

butler_tasks item 行已有 📐 按钮复制"完整 schedule prefix + topic"（如
`[every: 09:00] 把今日日历汇总写到 ~/today.md`）— 适合迁移 / 备份场景。
但 owner 常做的是"基于现有 schedule pattern 新建相似 task"：
1. 看到「每天 9 点 standup」schedule
2. 想新建「每天 9 点 stretch」task
3. 需要 `[every: 09:00]` prefix 起手，topic 自己写

走 📐 拿到 `[every: 09:00] standup`，要先粘贴再删 `standup` 再敲新 topic
— 三步。

本迭代加 ⏰ 按钮：仅拷 prefix（不含 topic），让 owner 粘贴后接着敲新 topic。
与 📐 互补，覆盖"基于模板新建"workflow。

## Changes

仅 `src/components/panel/PanelMemory.tsx`：

- 在 📐 按钮之后插 ⏰ 按钮（与 📐 同 gate：仅 `catKey === "butler_tasks"
  && parsed` 时显）
- click → 计算 prefix 字符串（every / every_weekdays / once / deadline 四
  种 schedule kind 对应格式）→ `navigator.clipboard.writeText(prefix)`
- 成功 / 失败 setMessage 反馈 2.5s 自清；hover title 解释与 📐 互补关系

## Key design decisions

- **专用按钮而非复用 📐 + 修饰键**：⌥click / shift+click 等修饰键变种不可
  发现 — owner 必须知道才会用。两个独立按钮 emoji 区分（📐 ruler =
  完整模板，⏰ alarm = 仅时刻部分）让两条工作流都自文档。
- **复用既有 parsed schedule 数据**：parsed 已是 `parseButlerSchedule` 输出，
  内含 kind / hour / minute / weekday mask / year-month-day 等所有字段；
  直接拼字符串不需要新 IPC。
- **与 PanelTasks "📐 调期" popover 独立**：那个调既有 task 的 due 字段；
  本按钮是给 butler_task 起手模板用，二者不冲突也不复用。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.21s)
