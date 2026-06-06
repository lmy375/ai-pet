# 024 · memory 主动遗忘 propose — 自我进化的"遗忘"通道

PanelMemory 当前 prune 通路只有 user 显式删 + 30d butler archive + 14d session_distill 撤回窗。23 落地后 distill 自动写入会让列表加速膨胀，但 pet 没有"主动提议忘记"的通道。GOAL「自我进化 / 记忆」缺这一对偶 — 007/018/023 是"记住"，遗忘也该是 pet 主动维度。

需求：
- 新 proactive 子项 `forget_propose`，周期触发（周一 18:00，与 021 mood weekly 错开），扫候选：age ≥ 30d + pin 数 0 + 30d 内未被对话引用 + 7d 内未被 007/018 命中。
- 候选送 LLM 判断 should-forget 置信；置信高的上限 5 条 bundle 成一次 propose："这几条好像过去很久了，可以忘了吗？" 列出。
- user 批准（自然语言 / 编号 / "全部"）→ 对应 item 标 forgotten 不物理删，从 PanelMemory 默认视图消失；TG `/forgotten` 可回查。
- user 拒绝 / 跳过：被拒条目 14d 内不再 propose；批准失败的同批不重提。
- 与 007/018 候选池冲突时让位"记住"路径：同一 item 不在被 propose forget 的同 tick 被 007 回访。
- 阈值（age / pin / 引用窗口 / bundle 上限）常量集中，不暴露用户面板。

---
实现笔记：
- 新建 `src-tauri/src/proactive/forget_propose.rs`：pure `select_candidates` 五重过滤（age ≥30d / 非 `[forgotten:]` / 非近 14d `[forget_declined:]` / 非 `[pinned]` / 不在近期 session text / 非 7d 内被 007 `follow_up` 命中），`MAX_BUNDLE = 5` 取顶。`format_forget_propose_intent` 教 LLM 列编号 + 协议「同意 → memory_edit append `[forgotten: YYYY-MM-DD]`，拒绝 → append `[forget_declined: YYYY-MM-DD]`」。9 单测覆盖各筛子 + intent 协议字段。
- proactive.rs `maybe_run_forget_propose` + `LAST_FORGET_PROPOSE_WEEK` ISO 周去重 + 周一 18:00 + 60min grace 三重 gate；触发后跑 LLM → emit 提议；不论 SILENT / 失败 / 成功都 mark 本周完成（避免 grace 内重试）。butler_history 记 `forget_propose` audit event；让位 mute。
- LLM-mediated 物化：用户回复 → LLM 用既有 `memory_edit` action=update append marker，无新 tool。decline cooldown 走 `[forget_declined: ...]` marker，被 select_candidates 14d 内过滤；新 7d 内 007 命中 → 跳。
- **缺口**：（1）PanelMemory 默认视图过滤 `[forgotten:]` 未做（前端 10K 行入侵风险）；当前 forgotten item 仍可在 PanelMemory 看到 description 末尾带 marker。（2）TG `/forgotten` 回查命令未做（GOAL 提到但作 nice-to-have）。（3）与 007/018 同 tick 冲突让位未显式实现 —— 007 任意 tick / forget 仅 Mon 18:00 grace，时间窗不重叠，加上既有 60s proactive cooldown 软兜底；如果实际观察到冲突再补硬约束。
