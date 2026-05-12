# PanelMemory butler_tasks "✏️ 改 schedule" 快速按钮

## 需求

butler_tasks 项目的 schedule（`[every: HH:MM]` / `[once: ...]` /
`[deadline: ...]`）改时间是高频操作（"早 9 点改到 10 点"）。当前要
点编辑 → 在 textarea 内手改 → 保存，三步且容易把 description 改坏。
补一键 ✏️ 弹小 modal 只改时间字段。

## 实现

`src/components/panel/PanelMemory.tsx`：

### state

- `editScheduleDraft: {title, description, kind, date, time} | null`
- `editScheduleBusy: boolean`（防双触保存）

### UI

- butler_tasks item 行 schedule chip 后插入 ✏️ 按钮（仅 parsed 命中
  时浮）
- onClick 把 parsed 拆成 draft：date / time 都是 string 直接绑 input
- modal 居中 380 宽：
  - kind 标签（只读，让用户知道当前是 every / once / deadline）
  - kind !== "every" → date input (YYYY-MM-DD)
  - 永远 → time input (HH:MM)
  - 取消 / 保存按钮
- 保存：
  - 校验 time 格式 `HH:MM` + date 格式 `YYYY-MM-DD`
  - 用 parseButlerSchedule 拿原 topic
  - 拼新 prefix：`[every: HH:MM]` 或 `[kind: YYYY-MM-DD HH:MM]`
  - new desc = `${newPrefix} ${topic}`
  - invoke memory_edit update + loadIndex 刷新

### 不改的

- kind / topic 都不在此 modal 改：那要走"编辑"全编辑器
- 不动 detail.md（仅改 description）

## 验证

- `cargo check` 不需（无后端改）
- `npx tsc --noEmit`：clean
- 行为：
  - butler_tasks 行的 schedule chip 旁见 ✏️ 按钮
  - 点击 → modal 弹开，time 字段预填当前 HH:MM
  - kind=every：仅显 time 字段
  - kind=once/deadline：显 date + time 双字段
  - 改 time → 保存 → memory_edit 更新 → loadIndex 刷新 → chip 显新时间
  - 时间格式错（如手输非法）→ 红字提示 + 不写
  - backdrop click / 取消 → 关 modal 不写
  - 改 schedule 失败 → 反馈
  - 无 parsed schedule 的 task → 按钮不浮

## 不在本轮范围

- 没让 modal 改 kind（如 once → deadline）：那是语义切换，应走编辑
  全编辑器
- 没改 topic 字段：topic 不在此 modal 范围
- 没 batch 改多条 schedule：bulk 是另一 axis，scope 大
- 没集成 schedule 模板（如 "每天 09:00"）下拉：现 time input 已直观

## TODO 池剩余

- PanelDebug "上次 manual fire 历史 ring" 显近 5 条
