# 018 · memory 回访升级为 task propose — 把"你还做吗"变"我帮你做吧"

007 让 pet 主动回访 ≥ 7d 的 memory item 中含未来意图的条目，但形式止于问句："上次你说想试新咖啡店，去了吗？" 当 intent 实际可由现有 tool 链解决时，止步问句是浪费。GOAL「管家」+ feedback「pet picks up work」直接落在这一步。

需求：
- 007 命中候选 memory item 时，LLM 在生成回访话术之前增一步判断：该 intent 是否能由现有 tool 链（URL fetch / file / memory / weather / calendar）落地解决。
- 可解决：回访话术升级为 propose 形态 — "上次你说想试新咖啡店，要不我帮你查几家评价高的？"；user 回复同意 → 自动落 012 deferred_task entry，spec 指向原 memory item。
- 不可解决（如旅行、需要现实行动）：退回 007 原问句形态，不强行 propose。
- user 拒绝 propose：不重试，同一 memory item 30d 内不再被 propose（仍可被 007 普通回访命中）。
- propose 命中转化的 deferred_task 在 014 PanelReports source-type chip 中标记 `proposed_from_memory`，可与普通 deferred 区分回查。
- 不引入新 panel / 新命令；纯 007 prompt 路径升级 + 012 落库 hook。

---
实现笔记：
- `proactive/memory_follow_up.rs::format_follow_up_intent` 末段加 GOAL 018 升级判断 block：教 LLM 先内心判断「可否由工具链落地」→ 可 → propose 形态 + 同意时调 `defer_task`，spec 必须以 `[from-memory: <原标题>] ` 前缀开头；不可 → 保留原问句。
- `tools/defer_task_tool.rs` description 同步给「from-memory tagging」一段，让 LLM 知道前缀协议存在 + 何时使用。
- `panel_reports.rs::ReportSource::ProposedFromMemory` 新变体：list_entries 在 source := Deferred 后**先 reclassify**（peek deferred store 看 spec 是否以 `[from-memory:]` 起手），再 filter，避免 "proposed_from_memory" filter 漏命中。Deferred + ProposedFromMemory 走同一 spec/output/success 推导分支 —— 区别只在 source 字段 + 前端 chip。
- 前端 `PanelReports.tsx` 加「💡 提议落地」chip + sourceIcon 分支；TG `/reports_list` / `/report` icon match 同步补 💡。
- "deferred" filter 同时包含 Deferred + ProposedFromMemory（后者仍属 deferred 大类）；"proposed_from_memory" 单独精筛；"all" 都通过。
- 缺口：「user 拒绝 propose → 30d 不再 propose」未实现。需要 LLM 显式调一个 `record_propose_declined` 之类的 tool 或 ai_insights memory 写入 + 拉黑列表读取；本轮先靠 prompt「拒绝不强推」依赖 LLM compliance，若实地观察反复滋扰再补硬约束。
