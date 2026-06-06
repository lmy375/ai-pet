# 041 · task 创建时 calendar 冲突预警 — 管家不该让你撞日程

016 morning_briefing 已在早安 enrich 中调 calendar tool 给当日日程，但 task 创建路径（reminder / 011 / 020 reminder 节点 / 012 deferred）完全不查日历 — 用户说"明天下午 3 点提醒我 X"，pet 静默落 reminder，与下午 3 点会议撞日程，user 当场才发现。memory feedback 已点名 calendar tool 已具备 / coordination 是 gap。

需求：
- 在 reminder / 011 scheduled / 020 chain reminder 节点 / 012 deferred 创建路径上，时间解析（含 022 ambiguity confirm 后的最终时间）落地前 pet 调 calendar tool 查 ±30min 窗口冲突。
- 命中冲突 → pet 反问："这个时间你日历上有 X，还要在那时提醒吗？" 给三个候选：保持 / 推迟 30min / 让 pet 重选合适时刻。
- 无冲突 → 静默落库，不加"已确认无冲突"占位 noise。
- 用户回复后正式落 task（推迟选项走 022 ambiguity 模块复用）。
- calendar tool 不可用 / 超时 → 退回原静默落 task 行为，并在 butler_history 加一行 `calendar_check_failed` 用于 audit。
- 仅在 task 创建瞬间 check；不周期重扫已落 task 的冲突（避免持续打扰）。
- ±30min 窗口阈值常量集中可调。

---
实现笔记：
- 新建 `src-tauri/src/calendar_conflict.rs`：参照 022 `inject_time_ambiguity_layer` 范式——纯 LLM-prompt 自约束流，**不引入新工具**（既有 `get_upcoming_events` 已就位）也不在 backend 拦截工具调用。常量集中：`CALENDAR_CONFLICT_WINDOW_MINUTES=30` / `DEFAULT_DELAY_MINUTES=30`（推迟候选默认偏移等宽于窗，保推迟后必出窗）。
- `inject_calendar_conflict_layer` 注入 system note：4 步协议——(1) 调 get_upcoming_events 查目标时间 ±30min；(2) 命中冲突给 3 候选（保持 / 推迟 30min / pet 重选 ±2h 内无冲突点）；(3) 无冲突 → 静默落库**不要**「已确认无冲突」占位 noise；(4) tool 失败 / 超时 → fall through 静默落 task **不要**因 calendar 故障阻塞。
- 与 022 区分明示：022 解决「说话模糊」（傍晚 / 下周末等）；041 解决「说话清晰但撞日程」。两层叠加生效不重复反问。
- `has_conflict_within_window` pure helper 暴露 pub 给将来 backend 拦截路径备用（当前未用，签名稳定）。
- 集成站点：commands::chat::chat（desktop） + telegram::bot::run_chat_turn（TG）——与 022 同 2 站。task 创建只发生在 user-facing chat 里，proactive emit 不落 user task。
- 9 单测：inject 位置（system 段后）/ 空 user messages 时 push end / 三候选关键字 / tool failure fall through / 反指令禁占位 / 创建瞬间 vs 周期重扫 / pure helper 三档（命中 / 远 / 边界等号）。
- **缺口**：
  1. **butler_history `calendar_check_failed` audit**：spec 写「tool 失败时 butler_history 加一行 audit」，本刀未做。原因：纯 inject layer + LLM-driven 流，backend 无法直接观察「LLM 调 tool 是否失败」。需要 wrap `get_upcoming_events` tool execute 或在 ToolContext 加 result-observer hook 后才能可靠 audit。Gap 留待 ToolContext 通用 audit 钩子工作。
  2. **推迟候选走 022 ambiguity 复用**：spec 写「推迟选项走 022 ambiguity 模块复用」，本刀未显式 wire。当前依靠 LLM 在反问 + 接收回复后自行调 todo_edit / schedule_report 落库，022 ambiguity 协议同 prompt 内自然兼容——但未单独验证两层叠加交互。
  3. **inject 站点覆盖**：仅 chat + TG 入口注入；如果有其它非 user-facing 入口能创建 task（例如内部 proactive subtask），未注入。当前可见路径里没有，但未来若添加需补 inject。
