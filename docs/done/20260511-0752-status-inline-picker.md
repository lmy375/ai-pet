# PanelTasks status badge 行内编辑

## 需求

priority badge 上一轮做了行内 picker。status badge 同模式 —— click pending
行的状态 → 弹"✓ 标 done / ✗ 取消"小 picker。"标 done"是最高频的单任务操
作，现在只能走键盘 `d` 或 bulk select，不够直观。

## 设计

不是所有状态都支持点编辑：

- **pending**：可以 → done / cancelled。这两条覆盖 90% 用户意图，picker 出 2 项
- **done / cancelled**：当前后端没有"标回 pending"路径（task_retry 只从 error
  转），picker 给不出有效选项 → 直接保持纯 `<span>` 不可点
- **error**：行已有"重试"按钮 → picker 多余 → 也保留纯 span

picker 只在 pending 行可见，避免给用户"我点了什么都没反应"的混淆。

## 实现

`src/components/panel/PanelTasks.tsx`：

- 新 state `statusPickerTitle: string | null`
- 共用 effect 同时处理 priority + status picker 的外点 / Esc 关闭（合并 listener
  生命周期）
- 状态 badge 在 `t.status === "pending"` 时渲为 `<button>` + 紧邻 popover；
  其它状态仍走原 `<span>`
- popover 两个 button：
  - `✓ 标 done` (绿 tint) → setStatusPickerTitle(null) + handleMarkDone(t.title)
  - `✗ 取消…` (红 tint) → setStatusPickerTitle(null) + handleCancelOpen(t.title)
    复用既有 cancelOpen / cancelReason / cancelInputRow 路径，让用户输 reason

复用 priority picker 的 stopPropagation 模式：button 与 popover 的 mousedown
都 stopPropagation，防外点 listener 自己关掉。

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - pending 行 → click status badge → 弹 2 项 picker
  - 点 ✓ 标 done → handleMarkDone（task_mark_done + reload）→ 行变 done badge
  - 点 ✗ 取消… → picker 关 + handleCancelOpen → 行下方既有 cancelInputRow 出现 → 输 reason → 确认取消
  - done / cancelled / error badge → 纯 span，不可点
  - 外点 / Esc 关 picker

## 不在本轮范围

- 没做 done → pending 回退 picker：后端缺路径。如果要加，得新加 task_set_status
  命令支持任意转换，需考虑事件历史 + butler_history 一致性
- 没做 status badge 的键盘可达：`d` 已经映射到 mark done，Esc 已能关 picker
- 没改 priority picker 的对应位置 —— 已经是独立 popover

## TODO 池剩余

- ChatMini G 快捷键跳到底
- /image prompt 历史模糊匹配
- 设置页"重置默认"按钮
- PanelMemory category sidebar hover preview
