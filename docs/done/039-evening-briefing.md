# 039 · evening_briefing — daily 节奏的另一半

016 morning_briefing 把日初管家问候做厚（天气 / 日程 / mood / 早安图 / 跨日 follow-up 036），但日末完全空白：pet 没有「送别」一天的节奏。daily_review.rs 是日终全天复盘归档（背后数据流），不是面向 user 的对话化送别。035 routine 学到的睡前时间窗正好可锚定 evening pacing — daily 节奏才闭环。

需求：
- 新 proactive 子项 `evening_briefing`，触发时间锚定 035 routine 学到的睡前时间窗前 30min（fallback 22:30）。
- 内容：当日完成 reminder / 020 chain 节点 / 012 deferred 数（"今天我帮你跑了 N 件事"）+ 关键事件（032 anniversaries 当日命中 / 034 surprise 今日发 / 037 goal 进度更新）+ 一句 mood-aware 收尾。
- 收尾句受 017 pet mood / 026 user stress 调：低 stress 走"辛苦了 / 早点睡"；高 stress 走"今天太累的话别管那么多"；中性走简短"明天见"。
- 当日完全无 fire-able 事件 → 退化为单句晚安，不强生成"今天没什么可说的"占位。
- 受 035 routine 睡前抑制约束：本 briefing 自身是睡前唯一允许的 proactive utterance；其他 proactive 在 evening 触发后到次日不再发。
- 不引入新 panel；不引入新 TG 命令；和 016 enrich 段落形成 daily 双锚点。

---
实现笔记：
- 新建 `src-tauri/src/proactive/evening_briefing.rs` pure helpers：`compute_target_time`(sleep_time-30min/fallback 22:30) / `should_trigger`(grace=90min + per-day dedup) / `EveningEventSummary` + `is_empty` / `count_today_events` (butler_history reminder + defer_done + chain_done，含负时区 offset) / `classify_closing_tone`(stress 优先于 mood，three-档 high/low/normal) / `closing_directive`(LLM 指令片段) / `format_evening_intent`(空事件返单句晚安模板 + 反指令禁「今天没什么可说」；非空列 N 件事 + anniversaries + surprise 提示) / `is_within_evening_suppression_window`(当日 fire 后至次日 04:00 抑制窗)。17 单测全覆盖。
- `proactive.rs::maybe_run_evening_briefing` 对偶 morning_briefing：mute → 取 routine effective_sleep_time → target+grace gate → 聚合 butler_history 当日事件 + anniversaries 今日 + surprise_log 当日 fired check → tone classify → run_chat_pipeline → SILENT 仍 mark fire → emit ProactiveMessage + LAST_EVENING_BRIEFING_DATE=today。
- 抑制窗共享态 `pub static LAST_EVENING_BRIEFING_DATE`：`run_proactive_turn` 入口 early-return 空 outcome；`maybe_run_surprise_gift` Gate 5 抑制检查（晚安后不该再蹦惊喜）。
- tick loop：evening_briefing 挂在 routine_learn 之后、evaluate_loop_tick 之前。
- **缺口**：
  1. **037 goal 进度更新**：spec 提到，037 未实现，`EveningEventSummary` 暂无 goals 字段；037 上线后加一行即可。
  2. **其它 proactive route 抑制**：仅 `run_proactive_turn` + `maybe_run_surprise_gift` 接入抑制窗 check；morning_briefing / memory_follow_up / deferred_task / scheduled_report / welcome_back 未加。morning_briefing 时刻通常 ≥04:00 抑制窗已自然结束，影响小；其它路线后续一行 helper 即可。
  3. **chain_done action**：butler_history 当下无 `chain_done` action（020 chain 完成走 `defer_done`），count 已兼容（match `defer_done | chain_done`），020 分粒度时一行 if 即可。
  4. **mood 读 timing**：本刀在 fire 前读 mood——LLM 跑完后可能更新；当下不重读（避免双读 + 简化路径），与 morning_briefing 同处理。
