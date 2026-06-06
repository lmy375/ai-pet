# 016 · morning_briefing 自主 enrich — 让早安变成真正的管家问候

feedback_pet_butler_direction 直接点名 calendar / weather tool「已具备但 coordination 是 gap」。morning_briefing 现在是固定 spec：不查天气、不查日程；003 仅在加 mood 图。早安播报作为管家最高频的触达瞬间，没有 enrich 是巨大浪费。

需求：
- morning_briefing 生成前，让 LLM 自主调 weather + calendar tool 拉两份数据：今日天气一句话 + 今天前 3 条日程（含时间 / 标题）。
- 数据回流后与 mood、最近 PanelMemory transient_note、即将到期 reminder 一起送入 briefing prompt。
- 输出形态：「早安 + 天气一句 + 今日日程 N 条（无则跳过此行）+ 当下心情语 + 003 早安图」，整体 ≤ 6 行。
- weather / calendar tool 任一失败：静默跳过对应行，不阻塞 briefing；失败计数累入 telemetry.rs。
- 不引入用户配置开关；不引入新 tool — 复用 feedback memory 提到的现有 calendar / weather tool 实现。
- 与 003 的 mood 图协同：图作为最后一行附在文末，与文字 enrich 并存不冲突。

---
实现笔记：
- `format_morning_briefing_intent` prompt 重写：明确「按顺序主动调用 get_weather + get_upcoming_events + memory_list」+ 6 行结构指令（早安 / 天气 / 日程×3 / 心情）+「工具失败静默跳过」失败处理 + 反罗列约束（"14:00 客户视频" not "日程 1: …"）。完全复用既有 tool（无新 tool）。
- 失败计数：`proactive/telemetry.rs` 加 `BRIEFING_WEATHER_FAIL_COUNT` / `BRIEFING_CALENDAR_FAIL_COUNT` AtomicU64 + `get_briefing_tool_fail_counts()` Tauri 命令。新 `BriefingFailCountingSink` 实现 `ChatEventSink::send_tool_result` 嗅探 `"error":` 字符串 → 命中 weather / calendar 时 bump 对应计数；其它工具 / 非错误 result 不打扰。`maybe_run_morning_briefing` 用它替换默认 `CollectingSink`（reply 仍走 `run_chat_pipeline` 的 Result return）。
- 与 003 mood 图 / 既有 mood_history 协同：图生成路径 / mood read 顺序完全不变 —— enrich 只动 prompt 层 + sink 类型。
