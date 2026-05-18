> 该文件是产品需求池，描述待开发的所有产品需求。AI 可通过自己对产品需求的分析、代码的分析提出新的产品需求(包括功能的实现、Bug 的修复、代码的重构等),之后进入开发流水线,开发完成后从该文件中移除。
> 1. 如果需求列表已空，则自主开始需求分析,代码分析，提出新的需求，每个需求就不超过 100 个字描述。每个需求一行。
>   - 不要云同步等需求。
>   - 不要专注模式相关的需求。
>   - 不要周报日报相关的需求。
> 2. 每个需求要是一个具体、有价值的、工作量适中的任务。任务要为了实现 `GOAL.md` 中的目标而制定。任务不能过度复杂，如果某个任务过于宏大，可以考虑放到 `GOAL.md` 中。
> 3. 每一条需求，在实现时，在 docs 中创建一个 `yyyymmdd-hhmm-title.md` 的文件。编写开发计划，记录开发结果。
> 4. 开发完成后将上面的文件移动到 `done` 中。保持本文件处于一个简洁的状态。如果这项任务完成了一个值得用户关心的产品亮点，将其更新到根目录下的 `README.md` 中。每次修改提交一个 git commit。
>

- TG `/audit_summary`：单命令聚合 5 大 audit 信号（pin streak / cat 活跃数 / idle 数 / 今日动过数 / 7d done 数）— sprint kickoff 一键视图。
- PanelTasks chip-bar「🏷 30d rename」chip：与既有 7d rename chip 并排显近 30 天 rename 数 — 长周期 refactoring 节奏。
- TG `/cat_top [N]`：按 cat items 总量 desc 列前 N — 跨 cat 容量对比 audit（与 growth/decay 活跃度正交）。
- PanelMemory cat-level chip-bar 上方加「⊕ cat-sort radio」选项：default / 7d 净增 / 最近 update 单选 — 替既有两 toggle 互斥但同时浮的状态。
- TG `/here_pin`：把当前 pinned 清单作 「pin context」 注入 transient_note 60min — 让 pet 接下来的 reply 更聚焦 owner 当前 pin 任务。















































