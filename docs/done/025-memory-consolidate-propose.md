# 025 · memory 合并 propose — 把零散同主题 item 缩成一条

024 让 pet 主动 propose 忘掉过时 item，但有些 item 不是过时 — 是同一件事被多次零散记录（"想试新咖啡店" + "查到 X 咖啡店开了" + "周六准备去试"），抛弃可惜，单独看又啰嗦。需要"合"的对偶通路，与"忘"互斥但互补。023 session_distill 落地后同主题聚簇会更密集，这一通路尤其必要。

需求：
- 新 proactive 子项 `consolidate_propose`，周期触发（周三 18:00，与 021 / 024 错开）。
- 扫候选：寻找 ≥ 3 条 memory item 文本相似度高 + cat 一致 + age 跨度 ≤ 90d + 未被 consolidated_into 过的 group。
- LLM 判断是否同主题；同主题 group bundle 成 propose："这几条好像在说同一件事，我合并成一条吧？" 列出原文 + 拟合并文。
- user 批准 → 新建 consolidated entry，text = LLM 整合 + 保留各源 ts 列表 + 各源 pin 数累加；原 N 条标 `consolidated_into=<new_id>`，PanelMemory 默认视图只显新条。
- user 拒绝 → 这批不再 propose 30d。
- 与 024 forget 互斥：同 item 同 tick 不双 propose；consolidate 优先（保信息）；024 候选池排除已被 consolidate 提名的 item。
- 阈值（相似度、group 大小、跨度）常量集中。

---
实现笔记：
- 新建 `src-tauri/src/proactive/consolidate_propose.rs`：5 个常量（`MIN_GROUP_SIZE=3 / MAX_AGE_SPAN_DAYS=90 / MIN_TOPIC_SIMILARITY=0.5 / DECLINE_COOLDOWN_DAYS=30 / MAX_GROUPS_PER_PROPOSE=2`）。pure `find_consolidate_groups` 每 cat 内贪心聚类：候选预筛（无 `[consolidated_into:]` / `[forgotten:]` / 近 30d `[consolidate_declined:]`）→ Jaccard char 相似度阈值合并 → size ≥ 3 + age span ≤ 90d 过滤 → size desc 取 top 2 group。`format_consolidate_propose_intent` 教 LLM 三步协议（建新条 + 原 group append `[consolidated_into:]` / 拒绝 group append `[consolidate_declined:]`）。10 单测覆盖各 filter 分支 + intent。
- proactive.rs `maybe_run_consolidate_propose` + `LAST_CONSOLIDATE_PROPOSE_WEEK` ISO (year, week) 去重；周三 18:00 + 60min grace 三重 gate。butler_history `consolidate_propose` audit event 含 group / item 计数；mute / 失败 / SILENT 都 mark 本周完成。
- LLM-mediated 落库：用户回复 → LLM 用既有 `memory_edit` action=create + update append marker，无新 tool。整合文 / cat / ts 列表都交给 LLM 决定，spec 教协议留具体内容给模型创意。
- 与 024 错峰：024 Mon 18:00 / 025 Wed 18:00，同 tick 双 propose 物理不可能。两者 marker（`[forget_declined:]` / `[consolidate_declined:]` / `[forgotten:]` / `[consolidated_into:]`）前缀完全不同，互不污染。
- **缺口**：（1）024 select_candidates 没显式排除正被 consolidate 提名的 item —— Mon/Wed 错峰下场景不会触发，观察到再补。（2）PanelMemory 默认视图过滤 `[consolidated_into:]` 与 024 同款前端 gap。
