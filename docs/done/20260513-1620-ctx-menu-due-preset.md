# PanelTasks ctx menu 加 due preset 入口

## 需求

设 due 时间靠任务卡的 datetime input picker —— 鼠标 click 进 calendar
+ 选时间 三步以上。常见诉求"今天下班前"或"明早第一件事"用 preset 一
键就 done。补 ctx menu 两条 preset 入口。

## 实现

`src/components/panel/PanelTasks.tsx` 在 ctx menu "⚡ 标 NOW" 按钮后
插入 2 条 preset 按钮（仅 pending / error 可点的任务才浮 — 与 canMarkDone
gate 共用）：

- ⏰ due 今日 18:00（dayOffset=0, hour=18）
- ⏰ due 明日 09:00（dayOffset=1, hour=9）

onClick 闭包计算当前日期 + offset → 拼 datetime-local 字符串
（`YYYY-MM-DDTHH:MM`）→ invoke task_set_due → reload + setBusyTitle
管 in-flight 防双触。失败 setActionErr 反馈。

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - 右键 pending 任务卡 → ctx menu 显两条 due preset 选项
  - 点 "⏰ due 今日 18:00" → 任务的 due 字段 = 今天 18:00 ISO 字符串
  - 任务卡 due chip 即时显新时间
  - 已 done / cancelled 任务 → preset 不浮（canMarkDone false）
  - 凌晨 1 点点 "今日 18:00" → 仍是今天的 18:00（已过的话用户应该选明日）
  - 网络 / 后端失败 → actionErr 红字提示

## 不在本轮范围

- 没做"已过期 preset 提醒"：如凌晨 1 点选"今日 18:00"系统不提示；用
  户自己判断
- 没做更多 preset（"今晚 22:00 / 下周一 09:00"）：当前 2 条覆盖 80%
  场景；更多走 datetime picker
- 没让 preset 可配（localStorage 自定义时段）：经验值固定，配置需要
  UI 重
- 没把 preset 复用到 bulk action：bulk due 有 input 字段，与单个不同
  UX

## TODO 池剩余

- PanelTasks 导出 visible markdown 加 include detail toggle
- PanelChat "💾 保存为模板"
- PanelMemory "📋 复制今日 todo"
