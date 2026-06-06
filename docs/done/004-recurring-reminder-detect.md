# 004 · Reminder 周期化探测 — 自我进化 / 通用任务支柱

GOAL.md「自我进化：随着交互过程加强对用户的了解」+「通用任务」目前在 reminder 子系统是空白：用户连续多天手动设同形 reminder（如「明早 8:00 吃药」），系统每次都当全新 entry 处理，宠物对用户作息毫无积累。

需求：
- 在 `proactive/reminders.rs` 已有的提醒落地路径上，按 text-similarity + 触发时间窗口聚类近 14d reminder。
- 同一 cluster 命中 ≥ 3 次后，宠物在下一次落地该 reminder 时追加一句提议：「这条最近 N 天都出现，要我直接帮你每日 HH:MM 自动起吗？」
- 用户同意后写入新的「recurring」类型 reminder（与单次 reminder 共存，schema 上区分）；不同意则记下 declined 标记，30d 内不再追问同一 cluster。
- 自动周期化的 reminder 行为与现有 stale_reminder_hours 清扫兼容，不污染 PanelTasks 既有列。
- 聚类阈值（相似度、命中次数、窗口）做常量集中可调；不暴露给用户配置界面。

---
实现笔记：
- 新建 `proactive/reminder_cluster.rs`：Jaccard 字符集相似度 + ±30min 时段桶 + 同日去重的 cluster 算法；4 常量集中。`build_reminders_hint` 每次 due-now 触发 fire-and-forget 把 `reminder` 事件写 butler_history（per-(title, date) dedup），构成聚类数据源；新 async wrapper `build_reminders_hint_with_proposals` 读历史 → 聚类 → ≥3 命中时把提议拼到 hint 尾，主 proactive prompt 自然吃到。
- 「recurring」类型用既有 schema：`parse_reminder_prefix` 新增 `[recur-daily: HH:MM]` 分支，解析后退化为 `TodayHour` 让 due 窗口自带「每天命中一次」语义；stale 清扫只看 `Absolute` 故周期化条目天然免扫。零新表 / 零前端改动 / 兼容 PanelTasks。
- UX 闭环走 LLM-mediated：提议文案明确「同意 → 你用 memory_edit 把这条改成 [recur-daily: …]」，由 LLM 在收到用户口头同意后调既有 memory_edit 工具落盘 —— 避开了 Rust 端做 NL accept-parse 的脆弱性。
- 缺口：「不同意 → 30d 内不再追问同 cluster」未实现。需要要么 LLM-side 写入 memory marker + 聚类读它过滤、要么新 decline tool。本轮先靠 prompt「时机合适才开口」+ LLM 判断软性节流；如观察到提议过密再补做。
