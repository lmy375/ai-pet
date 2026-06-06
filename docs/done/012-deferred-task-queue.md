# 012 · Deferred async task queue — "好，我等会儿弄完告诉你"

011 处理 cron-定时的周期任务；010 处理 vague task 的拆解。但用户给宠物最常见的"管家活儿"是 deferred 异步：「帮我看下 ~/Desktop/notes 里这周写过哪些 / 找一下我提过 'foo' 的对话 / 整理一下今天的 reminders」— 没指定何时，期待"等会儿做完告诉我"。当前要么强迫宠物当场跑（抢用户注意力），要么落 reminder（变"提醒你自己做"），都不是管家行为。

需求：
- 新 butler 子能力 `deferred_tasks`，与 reminders / scheduled_report / butler_schedule 并列；持久化到磁盘。
- LLM 在 user turn 检测到「deferred 性质 + 可由 tool chain 解决」的请求（短词："帮我 / 等会儿 / 有空 / 不急"），主动把 task 落 queue 并对话回："好，我弄完告诉你"。
- Pet 在 proactive tick 中轮询 queue：选择 user idle ≥ N min / 非 deep-focus / 距上次主动 utterance ≥ M min 的窗口 fire 一条 deferred task。
- Fire 时 LLM 自由调用 tool 链（file / memory / butler_history / URL fetch 等）完成 → 输出结果作为 proactive utterance：「你早上让我查的 X，结果是...」。
- 失败 / 超时回退：「你早上让我查的 X 我没搞定，要换个问法吗？」，不无声丢；task 同步标 failed。
- TG `/queue` 查看待办 deferred task + `/queue_del <id>` 撤销；不引入 panel 面板，保持 conversational 入口。
- 与 011 区分：011 是 cron-触发的"定时跑活儿"，012 是 anytime-合适窗口的"找空跑活儿"；两者共用 tool 调用层与失败回退模板。

---
实现笔记：
- 架构镜像 011：新建 `src-tauri/src/deferred_tasks.rs`（JSON 落盘 `deferred_tasks.json` + `pick_oldest_pending` + `mark_finished` + 7d terminal 清扫）。新 LLM tool `DeferTaskTool`（`src-tauri/src/tools/defer_task_tool.rs`）注册到 ToolRegistry + BUILTIN_TOOL_NAMES；description 给「帮我 / 等会儿 / 有空 / 不急」短词作触发线索 + 与 011 / butler_task_edit / todo_edit 的边界说明。
- 触发四重门：mute → MIN_GAP_BETWEEN_FIRES_MINUTES (30min) → input_idle ≥ MIN_USER_IDLE_SECS (300s) → since_last_proactive ≥ 60s。命中后 `pick_oldest_pending` 拿一条 → run_chat_pipeline 跑 → 成功 emit「你早上让我查的 X，结果是…」；empty/SILENT/err → fallback「你之前让我…我没搞定，要换个问法吗？」+ mark_finished(failed)。
- butler_history 记 `deferred_fired` event（含 id + 是否 failed + spec excerpt）给将来 audit；不污染 mood_history。
- TG `/queue` `/queue_del <id>` 与 011 的 `/reports` `/report_del` 同款 wiring；`format_for_listing` pending-first / 同段时间倒序，让用户一眼看「我刚塞的」+ 「最近完成的」。
- 状态保留：done / failed 默认保留 7d 让用户事后追问；超过的 entries 在下次 add_task 时自动 prune。
- 「deep-focus」gate 字面在 gate.rs 仍无（与 007 / 008 同结论），用 input_idle + since_last_proactive 近似覆盖「用户不在键盘前 + 宠物刚才没开口」的 quiet window。
