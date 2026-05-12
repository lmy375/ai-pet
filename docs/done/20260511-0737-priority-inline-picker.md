# PanelTasks priority badge 行内编辑

## 需求

TODO 原话是"直接拖卡片到 P0..P9 行间隙改 priority"。HTML5 drag/drop 实现成
本与"行间隙"hit-target 不直观（用户得知道间隙的精确像素位置），不如改成
"click priority badge → 选择 P0..P9"的轻量交互。目标语义保留：单条 priority
改动不必走 select → bulk → 输 N 三步。

## 实现

`src/components/panel/PanelTasks.tsx`：

- 新 state `priorityPickerTitle: string | null`，同时只一个 popover 浮起
- useEffect 在 picker 打开时挂 window mousedown / keydown(Esc) listener；外点 /
  Esc 关 picker
- `handleInlineSetPriority(title, priority)`：set null + invoke task_set_priority
  + reload；失败写 setActionErr 让用户看到原因
- priBadge 从 `<span>` 改成 `<button>`，保留视觉样式（border:none + cursor:
  pointer + fontFamily:inherit）；onClick 切 priorityPickerTitle；mousedown
  stopPropagation 防外点 listener 自己关掉
- 紧贴 badge 右下角浮一个 5×2 grid picker（P0..P9），active 行 tint-blue 高
  亮 + 禁点；mouseOver / mouseOut 切 hover bg

## 不在本轮范围

- 真"拖"语义没实现：dnd 库（react-dnd / dnd-kit）引入 ~30KB，对单字段改动不
  划算。如未来支持"任务排序" / "任务搬到其它列表"才考虑接 dnd-kit
- 没改 status badge —— 单独 todo 写到 TODO.md 让"状态切换"也行内编辑

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 任务行 P3 chip → click → 5×2 picker 浮 → 点 P0 → reload 后该任务变 P0
  - active 行（点 picker 时该 priority 就是 P3）禁点 + 蓝色高亮
  - 外点空白 / Esc → picker 关
  - 同 panel 切到别的 task click P 也行（同时只能开一个 picker）
  - 网络 / 后端错 → actionErr 红条显原因

## TODO 池清空 → 自主提案

按规则 #1，提出 5 条新需求（已写入 TODO.md）：

1. status badge 行内编辑（与 priority 同模式）
2. ChatMini G 快捷键跳到底
3. /image prompt 历史模糊匹配
4. 设置页"重置默认"按钮
5. PanelMemory category sidebar hover preview
