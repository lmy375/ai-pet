# PanelMemory butler_tasks "📌 今日要执行" filter chip

## 需求

iter #210 加了 schedule kind chip 行（every / once / deadline / none）
让用户按类型筛 butler_tasks。但更常见的诉求是"今天到底要做哪几件"
—— 这是跨 kind 的语义：
- every 永远命中（每天触发）
- once / deadline 仅当日命中
- none 不算

补一个"📌 今日要执行" chip 覆盖此谓词。

## 实现

`src/components/panel/PanelMemory.tsx` 在既有 chip 计算 IIFE 内：

- 加 `isTodayExecution(parsed)` helper：every → true；once/deadline 比
  year/month/day 三段
- 在原 4 个 chip 之前插 chip kind=`"today"` (icon 📌, 绿色 tint：
  `#dcfce7` / `#166534`)，放最左让"今天"做用户首要锚定
- 计数 `todayCnt` 累加：parsed && isTodayExecution
- filter 逻辑扩展：sentinel `"today"` 与 kind axis OR —— 多选 today +
  every 等于"今日要执行 OR 每天类"
- 不引入新 state；复用既有 butlerScheduleFilter Set 共享 OR pattern

## 验证

- `npx tsc --noEmit`：clean
- 行为：
  - 今天有 `[every: 09:00]` / `[once: 2026-05-13 ...]`（假设今天 5/13）
    → todayCnt 计入两条
  - 点 "📌 今日要执行" chip → 仅这两条可见
  - 加点 "🔁 每天" → 看到所有 every（包括今天）+ 今天 once/deadline
  - 点 "✕ 清除" → 全集恢复
  - 跨日：明天再开 panel，today 的 once/deadline 仍按 当时日期 判断
    （不需 backend；前端 new Date() 实时算）
  - 全部不命中 today（今天没排 once，全是历史 [once:]）→ count=0 跳过
    渲染（与其它 0 计数 chip 同款 silently skip）

## 不在本轮范围

- 没做"今日已执行 vs 今日未执行"细分：every 的 isButlerDue 在别处用
  作"过期 / 到期"判断；这里 today chip 只看 schedule 形态匹配今日，
  不看"是否已经过 fire 时点"
- 没集成"今日已 done"过滤：done chip（✅ 已完成）是单独维度，与 today
  filter 可叠加（用户多选）—— 当前 done 解析在 item 渲染时做，不进
  filter
- 没把 today chip 加到 PanelTasks（队列）：那是一次性派单，与 schedule
  概念不同

## TODO 池剩余

- PanelChat "查看全部标记消息" modal
- PanelTasks 任务卡 header history 摘要小字
