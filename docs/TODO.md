> 该文件是产品需求池，描述待开发的所有产品需求。AI 可通过自己对产品需求的分析、代码的分析提出新的产品需求（包括功能的实现、Bug 的修复、代码的重构等），之后进入开发流水线，开发完成后从该文件中移除。
> 1. 如果需求列表已空，则自主开始需求分析,代码分析，提出新的需求，每个需求就不超过 100 个字描述。每个需求一行。
>   - 不要云同步等需求。
>   - 不要专注模式相关的需求。
>   - 不要周报日报相关的需求。
> 2. 每个需求要是一个具体、有价值的、工作量适中的任务。任务要为了实现 `GOAL.md` 中的目标而制定。任务不能过度复杂，如果某个任务过于宏大，可以考虑放到 `GOAL.md` 中。
> 3. 每一条需求，在实现时，在 docs 中创建一个 `yyyymmdd-hhmm-title.md` 的文件。编写开发计划，记录开发结果。
> 4. 开发完成后，将上面的文件移动到 `done` 中。保持本文件处于一个简洁的状态。如果这项任务完成了一个值得用户关心的产品亮点，将其更新到根目录下的 `README.md` 中。
> 

- PanelTasks STATUS_BADGE 其余三档 + dueColor("soon") 配色迁 token：line 61 pending `#e0f2fe`/`#075985` → tint-blue；line 63 done `#dcfce7`/`#166534` → tint-green；line 64 cancelled `#f1f5f9`/`#64748b` → bg + muted；line 144 soon `#ea580c` → tint-orange-fg。status semantics 收尾。
- PanelToneStrip / PanelChipStrip / PanelPersona 红字提示迁 token：PanelToneStrip line 359/385、PanelChipStrip line 153/196、PanelPersona line 283 跨三个 panel 仍 hardcoded `#dc2626`，迁到 `tint-orange-fg` 与 R147~R153 全局警示一致。
