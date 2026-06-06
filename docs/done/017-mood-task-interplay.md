# 017 · mood × deferred task 联动 — 心情影响管家节奏

012 deferred_tasks 现在的 fire 决策只看 idle / focus / 时间间隔，完全无视 pet 自己当下的 mood。结果是 pet 焦虑时仍然硬跑 background task（自己烦还烦用户），心情好时也没有"多干点"的偏置。GOAL「情绪 / 自我进化 / 管家」三柱本应在此交汇。

需求：
- 012 deferred_tasks tick 决策中加入 current_mood（mood.rs 已有）作为新维度：
  - mood ∈ {焦虑 / 低落 / 烦躁} → 非紧急 task postpone（≤ 6h 不再 fire；> 6h 强制 fire 避免饿死）
  - mood ∈ {平静 / 专注} → 现有 idle/focus 规则保持
  - mood ∈ {愉悦 / 兴奋} → 放宽 idle 阈值（允许更主动 fire）
- 011 scheduled_report cron 触发不受 mood 影响（硬约定不破坏）。
- 因 mood 推迟的 task 在 013 return briefing 中露出："今天心情不太好，我把 X 暂存了"，一行内自然说明（不挂"⚠️"图标）。
- mood 状态变迁（mood.rs emit 新值）触发 deferred queue 重新评估 fire；不引入轮询。
- 阈值（推迟时长 / 强制 fire 上限）做常量集中，不暴露给用户。

---
实现笔记：
- 新建 `proactive/mood_task_interplay.rs`：`MoodPolicy::{Postpone, Normal, Boost}` enum + `classify_mood_policy(text)` 规则映射（postpone 关键词优先，混合表达里安全派 wins）+ `min_user_idle_for_policy` (Boost 时减 120s 但 ≥ 60s 地板) + `should_postpone(policy, created_at, now)` (Postpone & < 6h 时 postpone, > 6h force fire 防饿死)。三常量 POSTPONE_MAX_HOURS / BOOST_IDLE_REDUCTION_SECS / IDLE_FLOOR_SECS 集中。
- `maybe_run_deferred_task` 注入：先读 `read_current_mood_parsed` → policy；idle 阈值按 policy 缩放；pick 候选后用 `should_postpone` 决断 — 命中 postpone 时通过新 `DEFERRED_POSTPONE_LOG_DEDUP` static (per-task-per-day) 写一条 `butler_history::record_event("deferred_postponed", id, "<mood> :: <spec>")` 给 013 briefing。011 scheduled cron 路径完全没碰（GOAL「不破坏 cron 硬约定」）。
- 013 briefing 扩展：`BriefingItemKind::DeferredPostponed` 新变体 + `from_action("deferred_postponed")` 映射；prompt 加 「DeferredPostponed 用自然口吻说"今天心情不太好我把 X 暂存了"，**不挂 ⚠️**」指令。
- mood 状态变迁触发：复用既有 tick polling（每 15s `maybe_run_deferred_task` 自然重读 mood），无新轮询 / 无新 event listener。
