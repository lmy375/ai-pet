# 007 · 记忆回访 — proactive 翻旧账追问

GOAL.md「自我进化：随着交互过程加强对用户的了解」+「主动聊天」目前在 proactive 子系统是「时间触发 / mood 触发 / app duration 触发」三条线，但「memory 触发」缺席：用户一周前说「想试新咖啡店」、「计划买耳机」，宠物永远不会主动回访。PanelMemory 数据基础已齐，缺触发逻辑。

需求：
- 在 `proactive/` 新增 `memory_follow_up.rs`，与 reminders / morning_briefing / daily_review 并列。
- 触发期间从 PanelMemory 取 age ≥ 7d 的 item 候选，偏好：pin 数高、文本含未来意图词（"想 / 打算 / 计划 / 试试 / 准备"）、近 7d 未在对话中再次提及。
- 候选送一次轻量 LLM 判断「现在自然回访它是否合适」；返回不合适则跳过该 tick，不退回原 proactive 路径硬塞。
- 命中后输出作为本次 proactive utterance，话术由 LLM 围绕 item 生成（非模板「上次你说...还吗」式）。
- 每个 memory item 设 7d 回访冷却 + 命中后顺手在 butler_history 记一行 follow_up event；不污染 mood_history。
- 与 gate.rs 已有的 deep-focus / app-duration hard-block 串联：被 gate 屏蔽时此触发同步抑制。

---
实现笔记：
- 新建 `proactive/memory_follow_up.rs` 放纯函数（`future_intent_score` / `select_candidate` / `parse_followup_cooldowns` / `format_follow_up_intent`），proactive.rs 加 async wrapper `maybe_run_memory_follow_up` 走 IO + LLM。三层硬门控：mute → 全局节流（3h）→ per-item 7d cooldown。
- LLM「轻量判断 + 自然话术」**合并到一次 chat pipeline 调用**：prompt 显式给 `[SILENT]` 退出口，宠物认为不合适回访就直接 silent skip，不走两次 LLM 节省 cost。命中 → `butler_history::record_event("follow_up", title, ...)` 落盘，下 tick 由 `parse_followup_cooldowns` 直接读出，不引新文件 / 不引新 store。
- 两处与 GOAL 字面有偏差：（1）pin 数高加分 ——  MemoryItem struct 没有 pin 字段，本轮不引入 schema 变更，先靠 intent 词频 + recency 排序；（2）deep-focus / app-duration hard-block —— gate.rs 当前没有这两条具名 gate（grep 验证），改用 mute + 全局节流近似覆盖。
