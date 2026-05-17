> 该文件是产品需求池，描述待开发的所有产品需求。AI 可通过自己对产品需求的分析、代码的分析提出新的产品需求(包括功能的实现、Bug 的修复、代码的重构等),之后进入开发流水线,开发完成后从该文件中移除。
> 1. 如果需求列表已空，则自主开始需求分析,代码分析，提出新的需求，每个需求就不超过 100 个字描述。每个需求一行。
>   - 不要云同步等需求。
>   - 不要专注模式相关的需求。
>   - 不要周报日报相关的需求。
> 2. 每个需求要是一个具体、有价值的、工作量适中的任务。任务要为了实现 `GOAL.md` 中的目标而制定。任务不能过度复杂，如果某个任务过于宏大，可以考虑放到 `GOAL.md` 中。
> 3. 每一条需求，在实现时，在 docs 中创建一个 `yyyymmdd-hhmm-title.md` 的文件。编写开发计划，记录开发结果。
> 4. 开发完成后将上面的文件移动到 `done` 中。保持本文件处于一个简洁的状态。如果这项任务完成了一个值得用户关心的产品亮点，将其更新到根目录下的 `README.md` 中。每次修改提交一个 git commit。
>

- TG bot `/search_yesterday <kw>` 命令：限定昨日 updated_at 的 task 内 fuzzy 搜 — /search_today 的昨日对偶。
- TG bot `/alarms_today` 命令：今日触发的 reminder 集中视图 — 既有 /alarms 全量的 today 切片。
- PanelTasks 行 hover 「⏱ 历经 N 天」chip：done task 显 create→done 持续天数 — task 历程量化。
- detail.md 编辑器「⌘⇧M」插 markdown table 3x3 模板 — owner 快速搭表格架构。
- ChatMini 气泡右键「📝 转 reflect」菜单项：把气泡内容存为 ai_insights memory item — 与 💾 转 task 对偶（task vs 反思）。
- PanelDebug 「📊 7d LLM 调用 sparkline」chip：近 7 天每日 LLM round 数 mini-chart — 长视角节奏。





































