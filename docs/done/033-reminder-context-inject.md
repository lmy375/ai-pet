# 033 · reminder 触发附上下文 — 把空 ping 升级为带记忆的提醒

reminder 现在 fire 时只是裸文案"提醒：买菜"，但 pet 已积累的相关 memory（"用户上次说想买西红柿和鸡蛋"、"上周提过缺酱油"）完全没派上场。空 ping vs 带上下文一句话，对"宠物了解我"的感受差异巨大。GOAL「了解用户 / 通用任务」直接对应。

需求：
- reminder 到点 fire 前，LLM 用 reminder text 关键词在近 30d PanelMemory + session_distill（023）中 retrieve top-3 相关 item。
- 命中 → reminder 文案附 1 行上下文，例：「提醒：买菜。你上次说想买西红柿和鸡蛋。」总长 ≤ 2 行。
- 未命中 → 原 reminder 文案不加，不强塞"无相关上下文"占位。
- retrieve 失败 / LLM 错误：静默退回原文案，不阻塞 ping。
- 011 scheduled_report / 012 deferred_task fire 输出已含自由 LLM 调度 → 不再走本路径（避免重复 context）；仅作用于 reminder + 020 chain 的 reminder 节点。
- 用户对 contextual reminder 反馈"信息太多 / 简单点" → 命中 019 persona style 后整体禁用该模式（不退到逐条调）。
- retrieve 窗口 / top-N / 长度上限常量集中。

---
实现笔记：
- 新建 `src-tauri/src/proactive/reminder_context.rs`：纯后端检索（非 LLM-tool 自取）——`RETRIEVE_WINDOW_DAYS=30` / `TOP_K=3` / `CONTEXT_SOURCE_CATEGORIES=["ai_insights","user_profile"]`（023 session_distill 也写这两 cat；029 self_note 故意排除——pet-owned 非用户语境）/ `CONTEXT_LINE_CHAR_CAP=60`。
- 检索算法：`tokenize_topic` 切中文 2-gram + 空白英文 token（单 char ascii 跳过避免噪音）；`score_item` 在 (title + description) 数 token 出现次数；`top_k_related` 按 score 降序 stable sort + 过滤 score=0；`item_snippet` 脱 `[xxx:]` markers + char cap。
- persona disable：`PERSONA_DISABLE_KEYWORDS` 列「提醒别带上下文 / 提醒简单点 / ...」共 7 条；`is_context_disabled` 扫 `communication_prefs::active_preferences_desc` 命中任一即整体跳过 enrich（spec 反指令「不退到逐条调」）。
- 集成点：`proactive.rs::build_reminders_hint_with_proposals` 在 base hint 后、cluster proposal 前 append `format_context_block`。新增 `collect_due_reminder_topics` 无副作用扫 due topics（与 `build_reminders_hint` 不同，不重写 REMINDER_LOG_DEDUP / 不写 butler_history，避免重复 log 同条 reminder）。
- 13 单测：tokenize × 3 / score × 2 / top_k 过滤 + cap / 窗口边界 30d + 空 ts 优雅退化 / snippet 脱 marker + fallback title / format 空命中短路 + 含「不要罗列」反指令 / persona disable keyword 检测。
- 011 / 012 路径不受影响：本 enrich 仅作用于 build_reminders_hint_with_proposals，与 scheduled_report / deferred_task fire 是独立 emit 链路；spec「不重复 context」自然满足。
- **缺口**：020 chain reminder 节点未单独验证——chain 节点本身仍是 todo cat `[remind: ...]` entry，自然走同条 path，但未单测 chain 场景；session_distill cat 名变化时常量集中处一行可改，无 magic string 散落。
