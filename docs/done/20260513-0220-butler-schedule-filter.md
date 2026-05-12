# PanelMemory butler_tasks 段 schedule 类别 chip 过滤行

## 需求

butler_tasks 段长期累积会同时含每天循环（`[every:]`）、单次定时
（`[once:]`）、截止前提醒（`[deadline:]`）+ 无 schedule 散条。用户
管理时想"我现在只想看 every 类"或"看下还有哪些是 deadline 没处理"，
需要按 schedule kind 快筛。给这段加一个 chip 过滤行。

## 实现

`src/components/panel/PanelMemory.tsx`：

- 新 state `butlerScheduleFilter: Set<string>` + `toggleButlerSchedule`
  helper（与 PanelTasks tag filter 同模式）
- 不持久化 —— 过滤是即时阅读偏好，下次打开 panel 自然回到全显
- 在 cat.items.length > 0 + catKey === "butler_tasks" 时浮 chip 行：
  - 一次性扫 cat.items，统计 every / once / deadline / none 四档计数
  - 渲染各 chip：仅 count > 0 才显（空类不占位）
  - icon + 配色与 R80 schedule chip 一致：🔁 蓝（every） / 📅 黄（once）
    / ⏳ 红（deadline）/ 🔢 灰（none = 无 schedule 散条）
  - chip 点击 toggle 加入 / 移除 filter set
  - 多选 OR 命中（同 tag filter）
  - filter set 非空时浮"✕ 清除"按钮
- 在 items 计算路径前插一段 schedule 过滤：catKey === "butler_tasks"
  且 set 非空时按 kind 命中 filter。"none" sentinel 命中无 schedule
  的 item（与 iter #190 PanelTasks tag "无 tag" 同 sentinel 思路）

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - butler_tasks 段空 → 不显 chip 行
  - 全部任务都有 schedule 前缀 → 仅 every / once / deadline 三 chip 显，
    none chip 计数 = 0 跳过
  - 点 🔁 每天 → 仅 [every:] 任务可见
  - 同时点 🔁 + 🔢 → 显 every 与无 schedule 两类（OR）
  - 点 ✕ 清除 → 全显
  - 其它 category 段（todo / ai_insights 等）不显 chip 行
  - 过滤后 pin 排序 / 折叠 / 删除等下游逻辑都基于 filtered items

## 不在本轮范围

- 没把 chip 行 sticky 在 section 顶部（滚动也保留）：当前在 section 内
  随 items 滚动；sticky 需 CSS 重做，scope 不大但本轮聚焦 affordance
- 没让 chip 按 "已过期 deadline / 临近 deadline / 距远 deadline" 细分：
  现 deadline 一档；细分需要按 deadlineUrgency 拆，UI 重
- 没让 chip 跨 panel 复用（PanelTasks 也有相同 schedule 概念）：PanelTasks
  队列不直接接 butler_tasks 内 schedule（队列任务 = 一次性派单），
  两个 surface 语义不同不混
- 没做"按 schedule kind 重置 / 批量改"：仅过滤显示，不动数据

## TODO 池剩余

- PanelTasks header 加"清除全部已结束（done / cancelled）"按钮
