> 该文件是产品需求池，描述待开发的所有产品需求。AI 可通过自己对产品需求的分析、代码的分析提出新的产品需求(包括功能的实现、Bug 的修复、代码的重构等),之后进入开发流水线,开发完成后从该文件中移除。
> 1. 如果需求列表已空，则自主开始需求分析,代码分析，提出新的需求，每个需求就不超过 100 个字描述。每个需求一行。
>   - 不要云同步等需求。
>   - 不要专注模式相关的需求。
>   - 不要周报日报相关的需求。
> 2. 每个需求要是一个具体、有价值的、工作量适中的任务。任务要为了实现 `GOAL.md` 中的目标而制定。任务不能过度复杂，如果某个任务过于宏大，可以考虑放到 `GOAL.md` 中。
> 3. 每一条需求，在实现时，在 docs 中创建一个 `yyyymmdd-hhmm-title.md` 的文件。编写开发计划，记录开发结果。
> 4. 开发完成后将上面的文件移动到 `done` 中。保持本文件处于一个简洁的状态。如果这项任务完成了一个值得用户关心的产品亮点，将其更新到根目录下的 `README.md` 中。每次修改提交一个 git commit。
>

- PanelTasks 📋 audit chip 加 per-signal click 触发对应 filter：📌→pinnedFilter / 💤→idleFilter — 让 chip 从信息变 navigation entry。
- TG `/here_status` 后做 `/here_until <HH:MM>`：把当前 transient_note 延长 / 缩短到指定时刻 — /here_extend 替代品。
- PanelMemory cat header 加「📅 最早 created」chip：单 cat 内 min(items.created_at) — 「这 cat 多老」cat 寿命 audit。
- TG `/done_streak_chart`：列近 30 天每天 done 数 sparkline 行 — /streak 文本数字的可视化扩展。

















































