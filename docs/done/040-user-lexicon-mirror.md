# 040 · user 语言风格 fingerprint 学习与镜像 — intimacy 的最后一公里

027 topic arc（subject 维度）+ 006 intent profile（action 维度）+ 019 persona self-tune（user 给 pet 的元反馈）已覆盖三大画像维度。但 user 自己反复用的特异表达 / 缩写 / 自创词 / 口头禅，pet 完全不学不用 — pet 永远说"标准话"，user 永远感觉 pet 不是"我这个圈子"。GOAL「了解用户 / 自我进化」中 intimacy 维度真未触达。

需求：
- 新 proactive 子项 `lexicon_learn`，每周一次扫近 30d user input（ChatMini + TG）。
- LLM 提炼出现 ≥ 5 次的特异表达：缩写 / 自创词 / 口头禅 / 特定 sentence opener（排除通用高频词）。
- 落入新 user_lexicon store，每条记 phrase + 用法举例 + 频次 + 最近一次 ts。
- 注入所有 LLM prompt 头部（chat / proactive / morning_briefing / 011 / 012 输出 etc.）；prompt 鼓励**自然采纳**，不强制 echo（避免做作）。
- 与 019 persona 区分：019 是 user 显式告诉 pet "你回话简短点"等元反馈；040 是 pet 隐式观察 user 自己的语言习惯然后在合适处 mirror。
- TG `/lexicon` 查看当前已识别 + `/lexicon_clear <id>` 撤回错识 + `/lexicon_pin <id>` 锁定不被覆盖。
- 用户反馈"别学我说话 / 这样别扭" → 命中 019 整体禁用 lexicon mirror（保留学习记录但 prompt 不再注入）。

---
实现笔记：
- 新建 `src-tauri/src/user_lexicon.rs`：参照 027 topic_arc 范式——`LexiconEntry {id, phrase, example, mention_count, first_seen, last_seen, pinned, cleared}` + `NewPhrase` LLM tool 入参 shape + `MAX_PHRASES=8` (比 27 略多) + `MIN_OCCURRENCES_FOR_LEXICON=5`（spec 硬约束门槛）。`replace_unpinned` 原子替换保留 pinned/cleared；`clear_phrase`/`set_pin` 软删/锁定；`active_phrases_desc`（count 降序）；`PERSONA_DISABLE_KEYWORDS` 6 条中英；`is_disabled_in_prefs` + `is_disabled` 扫 communication_prefs；`inject_user_lexicon_layer` 自动跳 disabled / 空集，注入「自然采纳不强制 echo」反指令；`format_lexicon_intent` 含入选门槛 + 反通用词反指令 + SILENT 退出口；`format_for_listing` 给 TG 展示。4 单测覆盖 active 排序 / disable 关键词中英 / 列表 active-first 顺序 / intent 协议字符串。
- 新建 `src-tauri/src/tools/set_user_lexicon_tool.rs`：LLM 周扫专用工具，参考 set_topic_arc_tool 范式，工具描述明示「ONLY when invoked by lexicon_learn scan prompt，不在常规 chat 调用」；JSON schema 约束 mention_count ≥ 5。
- 集成 `proactive.rs::maybe_run_lexicon_learn`：Tue 22:00 ± 60min grace + per-ISO-week dedup + mute gate。与现有 5 个周扫错峰（Sun 21:00 mood / Sun 22:00 topic_arc / Mon 04:00 routine / Mon 18:00 forget / Wed 18:00 consolidate）。空 session corpus → 仍 mark 本周避免 grace 重试。butler_history 记一条 `lexicon_learn` audit 行。
- inject 集成：11 个 chat pipeline 站点（proactive turn / morning_briefing / evening_briefing / memory_follow_up / surprise_gift / deferred_task / scheduled_report / topic_arc_scan / lexicon_learn 自身**不**自注入避循环 / TG run_chat_turn / desktop commands::chat::chat）—— replace_all 命中 8 个 `messages` 站，3 个 `chat_messages` / `augmented` 单独 Edit。lexicon scan 自己不自注入避输入污染（与 027 同理）。
- TG triad：`/lexicon` 列（含 ⚠️ 注入已关闭 chip + ✅/📌/🚫 chip + example）/ `/lexicon_clear <id>` / `/lexicon_pin <id>`。3 Tauri 命令暴露 panel 备用。
- **019 vs 040 区分**已在模块文档头部明示：019 是 user 显式元反馈（`set_communication_preference`）；040 是 pet 隐式观察 user 自己的语言习惯并 mirror（`set_user_lexicon`）。两条 LLM 工具不会混淆。
- **persona disable 半路通道关闭语义**：spec 写「保留学习记录但 prompt 不再注入」。本实现：is_disabled() 命中时 `inject_user_lexicon_layer` 早退 noop；`maybe_run_lexicon_learn` 仍跑（保留学习），不短路。
- **缺口**：
  1. **SCAN_WINDOW_DAYS=30 未严格生效**：当前用 `build_recent_session_corpus`（按消息条数 ~30 条限），不是真的 30 天滑窗。常量保留协议参考；后续 corpus builder 加 ts-filter 后启用。与 027 同款约束。
  2. **通用高频词排除**：靠 intent 反指令 + LLM 自约束，非 backend 黑名单。下次若发现仍有泄漏可加 stop-word list 在 `replace_unpinned` 入口过滤。
  3. **mirror 效果监测**：当下无 metric 衡量"pet 实际采纳频率 vs user 反馈"。是否需要后续 panel 加 mirror-hit chip 留观察。
