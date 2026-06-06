# 011 · 用户定制周期报告 — 让管家自己跑活儿

按 feedback_pet_butler_direction：tool 已齐（bash / file / calendar / weather / memory），coordination 是 gap。reminder 是"到点提醒你做"，morning_briefing / daily_review 是固定 spec。但用户对宠物说「每周五 17:00 给我一份这周复盘」、「每月 1 号汇总上月开销关键词」这种"按周期跑活儿"的管家本职行为目前没有入口。

需求：
- 新 butler 子能力 `scheduled_report`，与 reminders / butler_schedule 并列；状态持久化到磁盘。
- 用户在 ChatMini / TG 自然语言下达（"每周五 17:00 给我一份这周做了什么"），LLM 解析出 `(cron-like 表达式, report spec text)` 写入 store；解析失败时反问澄清。
- 到时间宠物自动 fire：以 report spec 为目标，自由调用现有 tool 链（memory / butler_history / mood_history / file 等）拉数据，输出报告文本，作为一次 proactive utterance 推送。
- 报告 LLM run 失败 / tool 不可用时退化为「这次报告没跑成，要现在重试吗？」短回退，不无声丢。
- TG 新增 `/reports` 列出所有已设 scheduled_report + 支持 `/report_del <id>` 删除；不引入 panel 配置面板（保持 conversational 入口为主）。
- 每个 report 落地时在 butler_history 记一行 `report_fired` event，供未来 audit 用。

---
实现笔记：
- 新建 `src-tauri/src/scheduled_report.rs`：`ReportSchedule` enum（Daily / Weekly / Monthly）+ JSON 落盘到 `scheduled_reports.json`。不引 cron 库 —— LLM 直接给结构化字段（kind + weekday/day + hour + minute），覆盖用户口语 99%。per-schedule 防重 cooldown：daily 23h / weekly 6d / monthly 27d。
- LLM 入口：新 tool `ScheduleReportTool`（`src-tauri/src/tools/scheduled_report_tool.rs`）注册到 ToolRegistry + BUILTIN_TOOL_NAMES；tool description 给出三种 kind 的语义与歧义时反问的指引，自然 NL→结构化字段映射。
- 周期触发：`proactive::maybe_run_scheduled_reports` 主循环每 tick 扫 store → due 的 entry 顺序串行触发；`run_one_scheduled_report` 跑一次 LLM（soul + format_report_intent，工具链全开），失败 / SILENT → fallback 文案「这次报告没跑成，要现在重试吗？」；无论成败 mark_fired 落盘避免本 tick 反复。
- TG：新 `Reports` / `ReportDel { id }` variants 入 TgCommand 三处（enum / name / parse） + bot.rs handler 两段（list / del）。/reports 友好兜底空 store；/report_del 走既有 missing-arg + format_command_error 模板。
- butler_history：每次 emit 后 `record_event("report_fired", id, "<schedule> · [fallback?] <spec 60c>")`，给 /timeline / 未来 audit 用。
