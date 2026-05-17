> 该文件是产品需求池，描述待开发的所有产品需求。AI 可通过自己对产品需求的分析、代码的分析提出新的产品需求(包括功能的实现、Bug 的修复、代码的重构等),之后进入开发流水线,开发完成后从该文件中移除。
> 1. 如果需求列表已空，则自主开始需求分析,代码分析，提出新的需求，每个需求就不超过 100 个字描述。每个需求一行。
>   - 不要云同步等需求。
>   - 不要专注模式相关的需求。
>   - 不要周报日报相关的需求。
> 2. 每个需求要是一个具体、有价值的、工作量适中的任务。任务要为了实现 `GOAL.md` 中的目标而制定。任务不能过度复杂，如果某个任务过于宏大，可以考虑放到 `GOAL.md` 中。
> 3. 每一条需求，在实现时，在 docs 中创建一个 `yyyymmdd-hhmm-title.md` 的文件。编写开发计划，记录开发结果。
> 4. 开发完成后将上面的文件移动到 `done` 中。保持本文件处于一个简洁的状态。如果这项任务完成了一个值得用户关心的产品亮点，将其更新到根目录下的 `README.md` 中。每次修改提交一个 git commit。
>

- PanelTasks 任务行右键加「📊 看 history timeline」popover：复用 task_get_detail.history 数据弹小 timeline 视图（与 TG /timeline 对偶）。
- TG bot `/blocked_by <title>` 命令：列阻塞 title 的 active task（与 /forks 反向：那个列被 title 阻塞的，本命令列 title 被谁阻塞）。
- PanelMemory 加「🗑 批量清空 cat」按钮（带 confirm token 防误触）：empty 整个 cat 的临时项 cleanup。
- ChatMini 选区 toolbar 加「📚 加到 ai_insights」按钮：选段 → memory_edit("create", "ai_insights") + 自动 title（与 📝 note 同 channel 但分流 ai_insights cat）。
- detail.md 编辑器加「📋 复制本节 + 子节」chip：H2 段含其下 H3 一并复制（既有 handleCopyHeadingSection 只复单节）。
- PanelDebug 加「⚙️ shell exit code 分布」chip：扫近 N 条 shell 调用 exit code 累计 — debug LLM shell tool 失败率。























