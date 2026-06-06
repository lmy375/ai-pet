# 021 · 每周 mood 报告（011 built-in 预置）— 把闲置 mood_history 派上场

mood_history.log 从 Iter 103 起每天写但仅在 prompt section 回读，UI 从未露出。011 scheduled_report 通路已通 — 自然扩展是加「built-in default reports」概念，出厂默认开一条 mood 周报，让 user 开箱即感受到 pet「了解我的情绪」。

需求：
- 011 引入 built-in default reports 子概念：一组宠物出厂自带的 scheduled_report，user 看不出与自定义有差异（同入口管理、同 PanelReports 归档）。
- 首个 built-in：mood weekly report，cron = `0 21 * * 0`（每周日 21:00），spec = "本周 mood 走向 / 哪天最好哪天最差 / pet 一句观察总结"。
- 输出基于 mood_history.log 7d 切片；输出走 011 既有 utterance 路径 + 014 PanelReports 归档（source-type 标 `builtin_report`）。
- user 通过 `/report_del <id>` 可删除 built-in；删除后下次 app 启动**不**自动恢复（尊重用户选择），但 TG `/report_restore_builtin` 一键恢复全部 built-in。
- mood_history.log 为空 / 缺该周时输出退化为「这周心情数据不够，暂时没法给你画像」，不强生成。
- 不引入 mood panel 可视化（图表 / 趋势线留给后续单独需求决定）；本需求只交付文字周报。

---
实现笔记：
- `ReportEntry` 加 `builtin: bool` 字段 + `ReportStore.deleted_builtin_ids: Vec<String>` 跟踪用户显式删过的 builtin id（serde default 都做了向前兼容）。`ALL_BUILTIN_IDS` 常量列表 + `builtin_entry(id)` 构造器作为 single source of truth；新增 builtin 时只需扩两常量。
- `ensure_builtins_installed()` 幂等安装：缺失 + 未在 deleted 集合时才补。`maybe_run_scheduled_reports` 每 tick 进 mute gate 前调一次（在 mute 前 — builtin install 与 mute 无关）。`restore_all_builtins()` 清空 deleted set + 再装一遍。`delete_report` 内部 `mark_builtin_deleted_if_applicable` 在删的同时记 deleted。
- 首条 builtin: `BUILTIN_MOOD_WEEKLY_ID = "rpt-builtin-mood-weekly"`, schedule = Weekly { Sun, 21:00 }, spec = "本周 mood 走向"。`run_one_scheduled_report` 检 `entry.builtin && id == BUILTIN_MOOD_WEEKLY_ID` 时走 `format_mood_weekly_intent` 而非通用 `format_report_intent`：预加载 mood_history.log 7d 切片直接嵌进 prompt，不依赖 LLM 拐去找数据。空 / 不足 → prompt 自身指令退化「这周心情数据不够」短文案。
- PanelReports `ReportSource::BuiltinReport` 新变体；list_entries 在 source = Scheduled 后看 `entry.builtin` 重分类。`"scheduled"` filter 同时包含 Scheduled + BuiltinReport（用户视角 "周期报告" 类目就该看到），`"builtin_report"` 单独精筛。前端 PanelReports.tsx 加「🎁 出厂自带」chip + sourceIcon 分支；TG `/reports_list` `/report` icon match 同步补 🎁。
- TG 新命令 `/report_restore_builtin`：清 deleted 集 + 重装。`/report_del <id>` 对 builtin id 起作用（已有路径），同时入 deleted_builtin_ids 避免下次启动复活。
