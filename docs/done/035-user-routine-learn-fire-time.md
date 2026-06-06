# 035 · user 作息隐式学习 → proactive fire-time 自适应

morning_briefing / proactive utterance 当前是固定时间触发 — 推早安时 user 还没起，推 deferred 时 user 正准备睡。004 是显式 reminder pattern 探测；隐式作息从未提炼。GOAL「自我进化 / 了解用户」缺这一面：pet 应该看着 user 输入行为推出作息，再让 fire 时间贴上去。

需求：
- 新 proactive 子项 `routine_learn`，每周一次扫近 30d butler_history 时间戳 + ChatMini 输入活跃度（不需新埋点，e61f83c active-app duration 数据可复用）。
- 提炼 user_routine：起床时间窗（每天首次输入活跃中位数 ± 30min）、睡前时间窗（每天末次活跃中位数 ± 30min）、工作高峰段。
- 落 user_routine store；旧值 EWMA 平滑，避免单日异常剧烈跳变。
- 应用：morning_briefing 触发时间从固定 cron 改为 "起床后 N min"，N 常量；proactive utterance 在睡前时间窗内仅允许紧急类（reminder / anniversary deadline 当日）。
- 数据不足时（< 14d 有效活跃数据）回退到固定时间默认值，不强求 routine fit。
- TG `/routine` 查看当前学习结果 + `/routine_set <key> <value>` 手动覆盖（如 user 想强制早安 7:00）。
- 不引入用户配置面板；不与 008 welcome-back idle 阈值耦合（idle 是绝对时长，routine 是绝对时刻）。

---
实现笔记：
- 新建 `src-tauri/src/user_routine.rs`：persisted `user_routine.json`；纯 helpers `group_by_date` / `median_time`（偶数样本取较小，避 23:59+00:01 wrap 插值 bug）/ `ewma_time`（α=0.3）/ `is_in_{wake,sleep}_window`（跨夜 circular 距离正确）/ `parse_event_timestamps`（含负时区 offset 边界）；`UserRoutine::effective_*` 走 override → learn(≥14d) → None 三档；`routine_get` / `routine_set` Tauri 命令。13 单测覆盖 group / median 各分支 / EWMA / 窗口跨夜 / effective 三档 / parser 时区。
- 数据源：butler_history.log 已就位事件流（reminder fire / cancel / snooze / follow_up / scheduled_report / ... 等都带 user-triggered 行为 ts）。spec 提到的 e61f83c active-app duration 数据当下无独立 series API，本刀以 butler_history 作"已就位最近的代理信号"，gap 段记录。
- 周扫：`proactive.rs::maybe_run_routine_learn` 周一 04:00 ± 120min grace + per-ISO-week dedup；纯数据 aggregate，**无 LLM 调用**——便宜可频繁跑、运行时不打扰 user（凌晨 4 点）。与 021 mood Sun 21:00 / 024 forget Mon 18:00 / 025 consolidate Wed 18:00 / 027 topic_arc Sun 22:00 错峰。
- TG `/routine` 显示 wake/sleep（含 🔒 override 标识 + sample_days + 学习状态）+ /routine_set 用法；`/routine_set <key> <value>` key 接 wake / sleep / clear_wake / clear_sleep，value `HH:MM`。
- **缺口**（this iteration 未做）：
  1. **morning_briefing fire-time 替换**：spec 写"固定 cron 改为'起床后 N min'"，本刀未触 morning_briefing 时间 gate（涉及 settings.morning_briefing.hour/minute 协议改 + grace 重算，影响面大）。`effective_wake_time` 已 pub 备用，下一刀一行注入即可。
  2. **proactive 睡前 gate**：spec 写"睡前时间窗仅允许紧急类"。`is_in_sleep_window` helper 已 pub 备用，但未在 evaluate_loop_tick 接入紧急 vs 非紧急分类。
  3. **active-app duration 数据**：未集成 e61f83c（找不到独立 series API），暂以 butler_history 作代理。
  4. **工作高峰段**：spec 提到"工作高峰段"未做（仅 wake/sleep 中位数）。需在 group_by_date 外加直方图 + peak 识别。
