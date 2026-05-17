# PanelTasks 任务行「📅 created N前」灰字 hint（< 3 天）（iter #306）

## Background

PanelTasks 行内有 🕰 老任务年龄 chip 显 created ≥ 3 天的 pending / error
任务（"积压"信号 → 提醒拆 / 改 priority / 取消）。但 < 3 天的"新进"任
务没有年龄信号 —— owner 不能一眼区分 "我刚 enqueue 的" vs "等了一天没
人理的"。

TODO 项「PanelTasks 任务行 hover 加「📅 created N天前」灰字 chip：让
owner 感知任务在队列里待了多久（积压信号 + 谁是新进）」—— 积压信号已
覆盖；本迭代补"谁是新进"。

## Changes

仅 `src/components/panel/PanelTasks.tsx`：

- 既有 🕰 老任务 chip 块改为双轨：
  - `ageMs >= 3 天` → 既有 🕰 chip（actionable bg / border 提示）
  - `ageMs < 3 天` → 新 "📅 N 前" 灰字 hint：
    - 无 bg / 无 border / opacity 0.7
    - monospace 字体（与 chip 体系视觉分量错开 — info vs actionable）
    - 文案直接复用既有 `formatRelativeAge`（"刚创建 / N 分钟前 / N 小时前"）
    - hover tooltip 显完整 ISO 时间

## Key design decisions

- **总是显（无 hover 触发）**：TODO 文字含 "hover" 但实践上 chip 行只有
  当 row 完全 hover 时才显额外信息会增加交互复杂度（需要 row class +
  :hover style 触发）。改为"始终渲染但视觉分量极低"（opacity 0.7 + 无
  bg）—— owner 扫描时 chip 行从左到右天然能识别，不必 hover；视觉噪音
  也可控因为新任务 chip 没有 bg。
- **复用既有 formatRelativeAge**：与 itemMeta "创建于" 同源 → owner 在
  task row chip 行 / 展开后 meta 行看到的"年龄"文案完全一致，认知一致。
- **3 天阈值不变**：保持与既有 🕰 chip 切换点对齐 — owner 心里一条线
  "3 天 = 老"已经建立，本次只是"< 3 天也显" 而非改阈值。
- **done / cancelled 不渲**：与 🕰 chip 同模板（静态终态年龄无 actionable
  信号；老归档 task 一堆 "3 个月前" 灰字噪音）。
- **monospace 数字 + 灰字 + 低透明度**：让 hint 视觉分量远低于
  actionable chips（📌 / 💤 / 🔒 等），让 owner 的注意力优先级仍在
  actionable chips 上 —— "新进信号" 是 info 维度，不抢眼。
- **不引 hover-only mode**：考虑过 row class + :hover .age-hint
  { display: inline-flex }，但 React 行管理 hover 状态需要新 state +
  onMouseEnter/Leave，复杂度不值；灰字 always-visible 已经够轻。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.19s)
