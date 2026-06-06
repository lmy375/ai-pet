# 015 · 对话内取消任务 — 管家应该听得懂"算了"

011 scheduled / 012 deferred / 现有 reminder 只能通过显式命令撤销（`/queue_del <id>`、`/report_del <id>`）。但用户脑子里改主意时说的是自然语言「不用了」「算了」「那个先别查」「取消刚才说的」。管家不能聆听撤回 = 半个管家。

需求：
- 在 user→pet turn 处理路径上让 LLM 能自主调用一个新的 `cancel_task` tool（args: type, id_or_keyword_or_recency_hint）。
- 候选来源覆盖：reminders / scheduled_report / deferred_task / butler_task 中状态非已完成的 entry。
- 取消范围模糊（如「那个先别查」）时，pet 反问 "是说取消 X 吗"，列最近 ≤ 3 候选；user 确认 / 编号选择即执行；2 轮内未澄清则放弃并提示用命令撤回。
- 用户语意精准时（"取消刚才那条 reminder"）直接执行不反问。
- 取消后在对应 store 标记 cancelled，并在 butler_history 记一行 `cancel` event；不物理删除（保留 audit 痕迹）。
- 014 PanelReports 出 cancelled chip filter，让取消记录可回查（不污染主列表默认视图）。
- 不引入新 panel / 新命令；这是 LLM tool 层的纯能力加法。

---
实现笔记：
- 新 LLM tool `cancel_task`（`src-tauri/src/tools/cancel_task_tool.rs`）注册到 ToolRegistry + BUILTIN_TOOL_NAMES。4 个 type 分发：scheduled_report → 新 `mark_cancelled`（store 加 `cancelled: bool` 字段，`is_due` 直接返 false 让 fire 停）；deferred_task → 新 TaskStatus::Cancelled 变体 + `mark_cancelled`；reminder / butler_task → memory_edit update 在 description 追 `[cancelled: <reason>]` marker（与既有 butler_task `[cancelled:]` 协议同源）。
- 物理保留：四条 path 都是 soft cancel，store 里 entry 不删；butler_history 加一行 `cancel <id-or-title> :: <task_type> :: <reason>` event 给 PanelReports 回查 + audit。
- PanelReports 改造：`ReportSource::Cancelled` 新变体 + `from_action("cancel")` 映射；filter "all" 显式**排除** Cancelled（GOAL「不污染主列表默认视图」），点 cancelled chip 才显。前端 `PanelReports.tsx` 加「🚫 已撤回」chip + sourceIcon 分支；TG `/reports_list` icon 也加 🚫。
- 缺口：「模糊时反问 / 2 轮内未澄清放弃」disambiguation 完全依赖 tool description 给 LLM 的协议指引（与 GOAL 015 写法一致），Rust 端没做显式 2-turn 计数。若实地观察到 LLM 不按协议（强行猜测 / 反复问），可后续加 inject 层强化或在 cancel_task tool 内对「单字关键词」自动返候选列表。
