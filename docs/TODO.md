> 该文件是产品需求池，描述待开发的所有产品需求。AI 可通过自己对产品需求的分析、代码的分析提出新的产品需求(包括功能的实现、Bug 的修复、代码的重构等),之后进入开发流水线,开发完成后从该文件中移除。
> 1. 如果需求列表已空，则自主开始需求分析,代码分析，提出新的需求，每个需求就不超过 100 个字描述。每个需求一行。
>   - 不要云同步等需求。
>   - 不要专注模式相关的需求。
>   - 不要周报日报相关的需求。
> 2. 每个需求要是一个具体、有价值的、工作量适中的任务。任务要为了实现 `GOAL.md` 中的目标而制定。任务不能过度复杂，如果某个任务过于宏大，可以考虑放到 `GOAL.md` 中。
> 3. 每一条需求，在实现时，在 docs 中创建一个 `yyyymmdd-hhmm-title.md` 的文件。编写开发计划，记录开发结果。
> 4. 开发完成后将上面的文件移动到 `done` 中。保持本文件处于一个简洁的状态。如果这项任务完成了一个值得用户关心的产品亮点，将其更新到根目录下的 `README.md` 中。每次修改提交一个 git commit。
>

- detail.md 编辑器 ⌘⇧F 全文 search & replace popover：既有 ⌘F 只查找，扩 ⌘⇧F 加 replace 半边（VSCode 风：搜框 + 替换框 + Replace / Replace All / ↑↓ 跳）。
- TG bot 加 `/timeline <title>` 命令：显单条 task description 中 [done] / [error:] / [snooze:] / [result:] 等 markers 时刻清单 — audit 这条 task 经历了啥。
- PanelChat 加「📌 bookmark 本条」按钮 + bookmark chip strip：让 owner 标重要 chat message + 顶部 chip click 跳回（与 task pin 对偶但 scope chat）。
- PanelTasks 任务行 hover 0.5s 显「✏」rename action chip：rename 当前藏在双击 title，新 chip 显式更易发现（与既有 hover preview tooltip 互补）。
- ChatMini 加「💾 导出本会话 markdown」按钮：与 PanelChat 既有导出对偶 — 小窗 owner 也能一键 export current session 到剪贴板。
- PanelMemory butler_tasks 段加「📊 schedule 24h 分布」mini bar chip：显各小时 fire 次数让 owner 一眼看分布偏态（早上 9 点扎堆 / 散布等）。























