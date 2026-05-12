# PanelPersona 当下心情 mini bar

## 需求

PanelPersona "当下心情" 卡只显当前一条 mood 文本 + motion glyph。下方的
"心情谱" 是 7-14 天 sparkline 跨日视角。中间缺一个"今天"维度的瞬时聚合
让用户看"宠物今天哪种情绪占主"。

## 实现

`src/components/panel/PanelPersona.tsx`：

- 复用既有 `moodDaily: DailyMotion[]` state（由 `get_mood_daily_motions`
  填，默认 7 天，末尾是今日）—— 无需新 IPC
- 在"当下心情" Section 末尾插一段 IIFE 渲染 mini bar：
  - 取 `today = moodDaily[length - 1]`；total === 0 不渲染
  - 4 motion 一行（Tap / Flick / Flick3 / Idle，与 MOTION_META 顺序一致）
  - 每栏：上半 colored bar（高 = count / max × 40px，至少 4px）+ 下半 glyph
    与 count
  - 0 计数走 muted border 色 + opacity 0.4 占位（保持 4 栏对齐感）
  - 颜色用既有 MOTION_META[*].color，与卡顶 glyph 配色一致
- 顶部加 11px subtitle "今日心情谱（X 次）"
- 与下方"心情谱"（多日 sparkline）形成时间梯度：当下 / 今日 / 近 7-14 天

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 今日还没有 mood 触发（total = 0）→ mini bar 不显
  - 今日只触发 Idle 3 次：4 栏，Idle 满高 + 灰底其它 3 栏占位
  - 今日均衡触发：4 栏比例相符
  - 每条柱 hover 显 tooltip "开心 / 活泼 (Tap) · 2 次" 等
  - 数据由既有 5s polling 通道自动刷新（mood-update 事件触发 fetchAll）

## 不在本轮范围

- 没改"近 24h 滚动窗口"语义：当前用"今日"（本地午夜起），与下方 sparkline
  的日历日聚合对齐。真正 rolling 24h 需要新后端命令；本轮的"今天哪种情
  绪占主"语义已被"日历日"覆盖
- 没做横向 trending（vs 昨日比例变化箭头）：今日 mini bar 是绝对快照；
  趋势走 sparkline
- 没区分 AM/PM：那是 sparkline 已有"早晚分段"开关，mini bar 保持简洁

## TODO 池剩余

- ChatMini 拖图到桌面气泡多模态
- PanelTasks task title 双击 inline 编辑
- PanelDebug timeline tab 切换
