# 008 · 用户回桌触发 — proactive 缺席的"欢迎回来"瞬间

GOAL.md「实时陪伴 + 情绪价值」中"用户从离开状态回来"这一最自然的陪伴瞬间，目前 proactive 子系统完全没接：existing 钩子只覆盖 app-duration / mood / time / 即将上线的 memory follow-up（007），但用户走开半小时回来坐下时，宠物保持沉默。

需求：
- 在 `proactive/` 新增 `welcome_back.rs`，监听全局输入 idle 时间（mouse / keyboard 无活动）。
- idle 累计 ≥ 30min 后，下一次用户输入恢复（或 ChatMini focus 回到前台）触发一次 welcome-back utterance；同一次离开-回来只触发一次。
- 文案由 LLM 围绕「离开时长 + 当下 mood + 最近 PanelMemory 中的 transient_note」生成，不走固定模板。
- 受 gate.rs 现有约束串联：deep-focus 期间不触发，rate limit 仍生效；与 morning_briefing 同窗口期内冲突时让位 morning_briefing。
- idle 阈值与冷却（默认 30min idle / 单次回桌 1 次 / 重复 idle 间隔 ≥ 2h）做常量集中，不暴露给用户配置。
- 落地与 e61f83c 的 active-app duration 共用底层输入事件源，避免另起一套监听。

---
实现笔记：
- 新建 `proactive/welcome_back.rs` 放纯函数（`should_fire_welcome_back` / `is_new_idle_session` / `format_welcome_back_intent`）；proactive.rs 加 async wrapper + 3 个跨 tick 静态（prev idle / fired_this_session / last_welcome_back_at），每 tick 滚动状态机。LLM prompt 给 `[SILENT]` 出口，让「刚回桌就 transient_note 说专心工作中」自然跳过。
- 让位 morning_briefing / memory_follow_up：检查 `clock.since_last_proactive_seconds < 60` → 跳过。两者刚 emit 过的窗口里 welcome-back 自动安静；下一 tick 再评估，对真人离开-回来分钟级粒度无感损失。
- 与 GOAL 字面有偏差两处：（1）"deep-focus" 不触发 —— gate.rs 仍无此 gate（007 实现时已 grep 验证），改靠 transient_note 注入到 LLM prompt 让模型自判；（2）"与 e61f83c active-app duration 共用底层" —— 用 `input_idle::user_input_idle_seconds()`（HID idle，已存在）；若 e61f83c 同源即一致，否则属轻微双源化但**不**引入重复监听。
