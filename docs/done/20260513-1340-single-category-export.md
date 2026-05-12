# PanelMemory 单 category 导出下拉

## 需求

R98 给 PanelMemory 加了 "📋 导出"按钮把全部记忆拼 markdown 复制。但
用户常只想导单段（如把 butler_tasks 段拼出来给同事看任务清单，不想
夹带 todo / ai_insights）。补一个下拉选 category 单独导出。

## 实现

`src/components/panel/PanelMemory.tsx` 在 "📋 导出"按钮后插入 `<select>`：

- value="" placeholder option "📋 单段…"
- options 从 `Object.entries(index.categories)` 过滤掉 `items.length === 0`
- label 显 cat.label + 计数
- onChange：
  - 拼 markdown：H1 段名 + 时间戳 + 各 item H2（title + 更新时间引语
    + body）—— 与全集导出同结构但单段
  - writeText + `setMessage` 反馈 3s
  - reset value="" 让用户能重选同段（与 schedule template 下拉同 pattern）
- disabled !index（loading 时）
- 视觉与 📋 导出按钮平级 (s.btn 同款样式)

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - 空 category（如 ai_insights 一条没有）→ option 不出现
  - 选 "butler_tasks (5)" → 剪贴板装该段 markdown + toast "已复制「butler 任务」段（5 条）"
  - 粘到外部编辑器 → H1 段名 + 5 个 H2 item 段
  - reset 后可重选
  - 与 "📋 导出"（全集）/ "💾 .md"（写文件）并存，三条路径互补：全集
    剪贴板 / 全集文件 / 单段剪贴板

## 不在本轮范围

- 没做单段保存到文件（💾 .md 类）：单段场景下用户多半是即时粘贴，
  不必走文件路径；future 想要也好加
- 没做"多段勾选导出"（如同时 butler_tasks + todo）：单段已覆盖最常
  见诉求；多选需 popover state
- 没把单段 export header 改成 frontmatter（yaml + body 两段）：当前
  markdown 形态直接可读，frontmatter 是写盘场景的诉求

## TODO 池剩余

- PanelDebug "立即开口" 加 "✏️ 编辑临时 prompt"
