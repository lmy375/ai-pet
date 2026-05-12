# butler_tasks "📐 复制完整 schedule prefix + topic"

## 需求

butler_tasks item 已经有 🔗（复制 ref token）、📋（复制 detail 全文）、
📝（复制本条整段 markdown）按钮。但有一个用例没覆盖：复制 "完整
`[kind: ...] topic` 文本"，方便：
- 迁移到另一台机器（粘到新建编辑器）
- 备份到外部 .md 文件
- 粘到 PanelTasks 派一次性变体（去除 schedule 自动执行）

补 📐 按钮。

## 实现

`src/components/panel/PanelMemory.tsx` 在 🔗 复制 ref 按钮后插入 📐
按钮：

- 仅 `catKey === "butler_tasks" && parsed` 时浮
- onClick 重建 prefix：
  - every → `[every: HH:MM]`
  - once / deadline → `[kind: YYYY-MM-DD HH:MM]`
- full = `${prefix} ${parsed.topic}`
- writeText + setMessage 2.5s 反馈（含前 40 字预览）
- 不带 raw description 中的 [error] / [done] / [result] 附加 marker —
  纯净复制 schedule + topic

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - 任务 `[every: 09:00] 早安日程 [error: 上次失败]` → 点 📐 → 剪贴板
    = `[every: 09:00] 早安日程`（错误段已剥）
  - `[once: 2026-05-15 14:00] 周末整理` → 剪贴板装完整 once 段
  - 无 parsed schedule 的 item → 按钮不浮
  - toast 显前 40 字预览

## 不在本轮范围

- 没做"复制 raw description（含全部 markers）"：raw 路径有 📝 整段
  markdown 按钮覆盖
- 没把它做成下拉（ref / schedule / detail 三选一）：分离按钮直观，
  下拉折损可发现性
- 没自动 detect 内容含 "「ref」" 时一并提示用户："是否一起复制 ref"：
  ref 与 schedule 是独立 axis，混合 prompt 反而混淆

## TODO 池剩余

- PanelChat 自定义模板 "🛠 管理" modal
