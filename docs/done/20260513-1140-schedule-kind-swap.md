# 改 schedule modal 加 kind 切换下拉

## 背景

TODO 池另一条 "PanelTasks 排序加 NOW marker 在前模式" 审查后发现 R94
已实现"NOW marker 永远浮顶（不论 sortMode）"—— 冗余。本 iter 改做
schedule modal kind switch（iter #229 的 follow-up）。

## 需求

iter #229 的"✏️ 改 schedule"modal 锁住 kind（every / once / deadline）
不可改。用户想从 [every: 09:00] → [once: 2026-06-01 09:00] 还得走"编
辑"全编辑器。补 kind 切换下拉，让单 modal 覆盖类型变换。

## 实现

`src/components/panel/PanelMemory.tsx` 改 schedule modal：

- 把 "kind: X（只能改时间...）"提示替换为 `<select>` 下拉
- 三档 option：🔁 every / 📅 once / ⏳ deadline（emoji 与 chip 配色
  保持一致）
- onChange 智能补值：
  - 切到 every → date 清空（every 不需要 date）
  - 切到 once / deadline 且 date 为空（从 every 切过来）→ 自动填今
    天日期，让用户少敲一段
  - 切回原 kind → date 保留原值（state 中持续保存）
- date input 仅在 kind !== "every" 时浮（既有逻辑保留）
- save 逻辑无需改 —— 已用 editScheduleDraft.kind 拼 prefix

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - `[every: 09:00]` 任务点 ✏️ → 看到 kind=every 下拉 + time only
  - 切 once → date 字段浮出 + 自动填今天
  - 切 deadline → date / time 保留
  - 切回 every → date 字段消失（仍保留在 state）
  - 改 time + 保存 → 拼 `[every: ...] topic` 或 `[once: date time] topic`
    或 `[deadline: date time] topic`
  - 触发 memory_edit update → loadIndex 刷新 → chip 显新 kind + 时间
  - 日期格式错（手输非法日期）→ 保存时校验红字

## 不在本轮范围

- 没让 modal 同时改 topic：那是语义切换，走"编辑"全编辑器更稳
- 没做"kind 变换后 description 校验"（如 once 必须未来时间）：当前
  仅格式校验；语义校验交给宠物自己做
- 没改 PanelTasks 同款（队列没 schedule 概念）

## TODO 池剩余

- PanelChat 输入框 chat prompt 模板下拉
