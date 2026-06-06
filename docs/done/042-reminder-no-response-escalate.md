# 042 · reminder no-response 升级追问 — 管家 vs 闹钟的分水岭

reminder 现在 fire 一次后任凭 user 是否看到都自然结束。但管家级别的体验：user 5min 没 ack（没关 / 没回 / 没 done / 没 snooze）→ pet 应该主动再追，"还没看到你，X 不会忘了吧"，到再没动静第三次就温柔放弃。当前完全静默 = 闹钟行为，与 GOAL「管家」错位。

需求：
- 新 proactive 子项 `reminder_escalate`，与 reminders.rs 协作。
- reminder fire 时设 ack_deadline = 5min；ack 来源：ChatMini 关掉通知 / TG 任意回复该 reminder / 显式 /done / 走 030 snooze。
- 超 ack_deadline 未 ack → fire 二次，文案加重："还没看到你，X 不会是忘了吧？"，ack_deadline 再设 10min。
- 二次仍未 ack → fire 三次更直接："X 再不动一下我就放弃了哦"，之后停。
- 受 017 / 026 gate：pet mood low / user stress 高时跳过 escalate（不在用户已压力时再追加压力）。
- 二次 / 三次 escalate 失败次数累入 telemetry.rs 用于后续 audit。
- 与 030 snooze 兼容：snooze 触发后清掉 escalate state；与 020 chain 节点兼容：chain 节点 reminder 同享 escalate 行为。
- 阈值 (5min / 10min / 总次数 3) 常量集中可调。

---
实现笔记：
- 新建 `src-tauri/src/proactive/reminder_escalate.rs`：常量集中 `FIRST_DEADLINE_MIN=5` / `SECOND_DEADLINE_MIN=10` / `MAX_FIRES=3`。`EscalationState {topic, fire_count, last_fire_at}` + 全局 `Mutex<Option<HashMap<title, state>>>`。Pure：`is_past_deadline` / `next_action -> EscalateAction {Wait, FireSecond, FireThird, Drop}` / `format_escalate_intent`（二次「不会是忘了吧」轻度关切 + 三次「再不动我就放弃了哦」温柔放弃，含 SILENT 退出口 + 反指令禁施压/说教）。`record_initial_fire`（or_insert idempotent）/ `mark_escalated`（fire_count++）/ `clear_for_title` / `clear_all_pending` / `snapshot`。telemetry `REMINDER_ESCALATE_FAIL_COUNT` AtomicU64。
- 13 单测覆盖 deadline 边界 / next_action 4 档 / format_intent 二次三次反指令 + 非法 count / record_initial_fire idempotent / clear_for_title 隔离。
- 集成 hooks：
  - `build_reminders_hint` 首 fire 入口调 `record_initial_fire`（or_insert 不重置已升级 entry）
  - `snooze_reminder_tool` 成功后调 `clear_for_title`
  - `cancel_task_tool` reminder type 路径调 `clear_for_title`
  - TG `handle_text_message` 入口调 `clear_all_pending`（粗粒度 ack）
  - desktop `commands::chat::chat` 入口调 `clear_all_pending`（粗粒度 ack）
- `proactive.rs::maybe_run_reminder_escalate`：mute → snapshot → 清 Drop entries → pick 第一条 FireSecond/FireThird → mood Postpone (017) / stress 高 (026) gate → format_intent → run_chat_pipeline（注 communication_prefs + user_stress）→ SILENT/失败仍 mark_escalated 避同 deadline 反复 retry + telemetry fail++ → 非 SILENT emit ProactiveMessage + mark_escalated。每 tick 最多 emit 1 条。
- 接入 tick loop：goal_check_in 之后挂载。
- 与 020 chain 兼容：自然满足——chain reminder 节点是 todo cat `[remind:...]` entry 走 build_reminders_hint 同 path。
- **缺口**：
  1. **ChatMini 关掉通知 ack**：spec 列作 ack 源，未做（需前端 ipc 调 `reminder_escalate::clear_for_title`）。后续加 Tauri command 暴露给前端。
  2. **粗粒度 vs 精准 ack**：本实现 TG/desktop 输入对所有待 escalate 都 ack。多条同时待 ack 时若 user 只想 ack 一条，需走 cancel/snooze 精准命名。
  3. **SILENT 时 mark_escalated 选择**：选择 mark 让 fire_count 仍累上（避同 deadline 反复 retry + silent fail 进 telemetry）；spec 未明确，偏保守。
  4. **重启清零**：state 内存 mutex 重启清掉。spec 未要求持久化；重启后 reminder 自然走 build_reminders_hint 重 record，可接受。
