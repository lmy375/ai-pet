# 036 · 跨日 mood 跟进 — 昨天的低落今天有人记得

016 morning_briefing 已 enrich 当下 mood / weather / calendar / 早安图，但缺「昨天看你不好今天怎样」这一跨日维度。021 周报粒度太粗（每周一次回看），008 welcome-back 只看离桌时长不看跨日情绪。结果是 pet 每天早安都从零开始，user 昨日的低落今日无人记得 — 陪伴感的细节漏点。

需求：
- 在 016 morning_briefing 生成 prompt 时，比对 mood_history.log 前 1-2 天 mood 与今晨预测 mood。
- 显著 negative-to-recovery shift（前日 ≥ 焦虑/低落 + 今晨回升）→ 早安自然加入一行关切，例："昨天看你不太好，今天感觉怎么样？"。
- 显著 positive-to-flat shift（前日愉悦/兴奋 + 今晨回平）→ 反向跟进，例："昨天心情挺好的，今天也保持住？"。
- 无显著变化（mood 平稳）→ 不加 follow-up 行，保持 briefing 简洁。
- 受 026 user stress 抑制：高 stress 期跳过 followup，避免追问加压。
- mood_history 缺前日数据（数据不足 / app 未启动）→ 静默跳过，不写"昨天没记录"占位。
- 与 035 routine / 016 enrich 协同：follow-up 行嵌入 morning_briefing 的 enrich 段落，不另起独立 utterance。

---
实现笔记：
- 新建 `src-tauri/src/proactive/inter_day_mood.rs` pure helpers：复用 017 `MoodPolicy {Postpone, Normal, Boost}` 三档作 Negative/Flat/Positive 语义，无新关键词字典。`dominant_policy(entries)` 多数票（并列偏 Postpone，与 classify_mood_policy「Postpone 优先」spirit 同）；`compute_shift(y, t)` → `InterDayShift {NegativeToRecovery, PositiveToFlat, NoShift}`；`format_followup_directive` 返 LLM 指令（非直接 emit 文本）含反模板 + 嵌入 enrich 段落硬约束。
- mood_history 一条 entry 形态 `<ts> <motion> | <text>`——motion 已是简化标签 + text 是自述；合并喂 classify 命中率高于单 text。
- 集成点：`proactive.rs::maybe_run_morning_briefing` 在 032 anniversary enrich 之后、AiConfig 之前；short-mode（low_distraction）时跳过整段（spec「高 stress 期跳过 followup」）；数据不足 → `build_inter_day_mood_directive` 自身返空串，caller is_empty 短路。
- 11 单测覆盖 dominant 空 / 多数票 / 三方并列 Postpone 优先 / Postpone+Boost 并列 / shift 各分支 / 数据不足 NoShift / directive 反模板 + 反延续 / NoShift None。
- **缺口**：（1）spec 写「比对前 1-2 天 mood 与今晨预测 mood」——实现仅比对 yesterday→today，未做"前 2 天滑窗"+"今晨预测"语义（今晨预测需求不明，当下用今日已有 entries 替代——pet 早安发出时通常昨晚 record_mood 已落 1-2 条，今晨在 briefing run 前还未自动 record，所以"今晨"实际取 "fired_at 后第一条 mood"；如果 briefing 先 fire，today entries 可能仅含已有今晨 mood）。后续若需"今晨预测"可加 `classify_briefing_intent_mood` 启发式从 yesterday + 今晨 weather 等推。（2）021 周报粒度协同未联动——spec 提到"周报粒度太粗"作问题陈述，不要求联动。
