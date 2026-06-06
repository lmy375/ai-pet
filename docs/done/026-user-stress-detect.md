# 026 · user stress detection — pet 看用户累时知道收敛

017 让 pet 自己 mood 低时收敛 deferred fire，但反方向缺席：当 user message 显示压力大（语气紧张、连续负面词、回复变短/变急），pet 仍按正常节奏推 deferred、按完整 spec 跑 enrich，反而火上浇油。GOAL「情绪价值」需要 pet 对 user 状态敏感，不止对自己心情敏感。

需求：
- 每次 user→pet turn 后，LLM 给 user message 一个 stress score（0-1）；滚动 4 turn 平均入 `stress_history`（与 mood_history 同级目录持久化）。
- score ≥ 阈值且持续 ≥ 2 turn → 进入"低打扰"模式（独立于 017 pet-mood 路径，两路径同时命中取更收敛者）：
  - 012 deferred_task fire 同步抑制（同 017 焦虑路径处理）
  - proactive utterance（007 / 008 / 013 / 018 / 024 / 025）改走共情语调 prompt，弱化"再做点啥"
  - 016 morning_briefing enrich 短模式（仅天气一行 + 早安 + 003 mood 图，跳过日程 enrich）
- stress score 回落 → 行为恢复，不写"恢复了哦"占位。
- 与 mood 独立持久化，避免与 017 混淆触发路径；两者均为 prompt 头部上下文。
- 阈值、平滑窗、模式切换持续时间常量集中。

---
实现笔记：
- 新建 `src-tauri/src/user_stress.rs`：pure `classify_user_stress(text)` 规则启发评分（负面情绪词 / 多感叹号 / 全大写 / 抱怨疑问 / 短叹词加权 clamp 0-1，与 006 skill_profile 同款 zero-LLM 风格）；`record_score` 落 `~/.config/pet/stress_history.log`（与 mood_history 同目录）；`read_recent_scores` + `in_low_distraction_mode`（avg ≥ 0.6 + 末 2 条都 ≥ 阈值，GOAL「持续 ≥ 2 turn」）；`spawn_record_from_content` fire-and-forget；`inject_stress_layer` 仅 low-distraction 时插共情口吻 system note；`is_in_low_distraction_mode()` 给 sync 决策点用。9 单测覆盖检测 / 解析 / mode 判定。
- chat.rs + bot.rs run_chat_turn 加 spawn_record_from_content（与 skill_profile 同位置）；10 个 `run_chat_pipeline` 站点全部叠加 `inject_stress_layer`（紧跟 019 communication_prefs 注入）。
- 012 maybe_run_deferred_task：`user_stress_high || mood_postpone` OR 合并，更收敛者赢；命中后走既有 `deferred_postponed` marker 与 013 briefing 一脉相承。
- 016 maybe_run_morning_briefing：low-distraction 时给 intent 末尾追加「短模式 override」note 教 LLM 跳 calendar enrich / 心情啰嗦行，只保留早安 + 天气 + 003 mood 图。format_morning_briefing_intent 签名未改 — 既有单测断言不破。
- stress 回落 → 行为恢复：每次 `is_in_low_distraction_mode` 重读 log，模式判定瞬时反映；inject_stress_layer 不在该模式下时为 no-op；无「恢复了哦」占位。
- 与 mood 独立：stress_history.log 与 mood_history.log 分文件 / 分逻辑；二者只在 012 deferred + chat pipeline injection 两个汇合点同时考量。
