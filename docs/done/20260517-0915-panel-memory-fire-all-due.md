# PanelMemory butler_tasks「🚀 全部跑」批量 fire due 按钮（iter #274）

## Background

butler_tasks section 已有「立即处理 (N)」按钮（仅 N >= OVERDUE_THRESHOLD_MIN
5 分钟超时时浮），点击触发一次 proactive turn —— 让 LLM 选**一条**处理。
"morning sweep" 场景里 owner 9 点开 panel，可能同时有 3 条 09:00 every 任
务都 due，单次 turn 只能选一条，剩下两条要等下次 proactive cycle。

本迭代加「🚀 全部跑 (N)」按钮：串行 invoke `trigger_proactive_turn_for_task`
处理每条 due（每条一次 LLM 调用），实时显进度。与既有"立即处理"互补 —
"立即处理"是一次 LLM turn 智能选；"全部跑"是 N 次 turn 每条都执行一遍。

## Changes

仅 `src/components/panel/PanelMemory.tsx`：

- **state**：
  - `fireAllArmed: boolean` + `fireAllArmedTimerRef` 3s 还原
  - `fireAllProgress: { total, done, failed } | null` 进度状态

- **`handleFireAllDue(titles)` 异步 handler**：
  - 空 titles → 友好提示
  - armed 二次确认（3s 自动 disarm）
  - 串行 for loop 调 invoke + 错误计入 failed
  - 进度实时更新到 fireAllProgress
  - 跑完 reload + 6s 自清 message

- **按钮 render**：在「立即处理 (N)」之后插「🚀 全部跑 (N)」：
  - 仅 butler_tasks 段 + dueTitles 非空（或 progress 进行中）时显
  - 蓝底（与红底「立即处理」区分语义 — 那是紧急 fix，本是 sweep）
  - armed 红字 5s；progress 期间显 "跑中 N/M（失败 K）"
  - tooltip 三态文案（idle / armed / progress）明确语义

## Key design decisions

- **串行而非 Promise.all**：trigger_proactive_turn_for_task 每次都跑一次完整
  LLM turn（含 chat history / proactive context 组装）。并行触发会让 LLM 在
  同 chat 内接到混乱顺序的 prompt + tool calls race（一条 task 还没处理完
  下一条已经发请求），结果丢失或重复。串行虽然慢（N × LLM latency）但语义
  正确。
- **与「立即处理」共存**：两个按钮语义不同 — 一个是"LLM 选一条"，一个是
  "N 条全跑"。owner 想要哪个根据场景选。
- **dueTitles 不过滤 OVERDUE_THRESHOLD_MIN**：「立即处理」按钮的阈值是
  "确保不会因为 1 分钟内刚 due 就紧急触发"。「全部跑」是 owner 显式手动
  操作 + armed 二次确认，不需要 threshold —— 1 分钟前刚 due 的也算 due。
- **progress 实时更新**：每条跑完 setFireAllProgress → 按钮文字立即变化
  ("跑中 1/3 → 2/3 → 3/3")。失败计入 failed 但不中断流程。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.22s)
