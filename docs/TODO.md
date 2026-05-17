> 该文件是产品需求池，描述待开发的所有产品需求。AI 可通过自己对产品需求的分析、代码的分析提出新的产品需求(包括功能的实现、Bug 的修复、代码的重构等),之后进入开发流水线,开发完成后从该文件中移除。
> 1. 如果需求列表已空，则自主开始需求分析,代码分析，提出新的需求，每个需求就不超过 100 个字描述。每个需求一行。
>   - 不要云同步等需求。
>   - 不要专注模式相关的需求。
>   - 不要周报日报相关的需求。
> 2. 每个需求要是一个具体、有价值的、工作量适中的任务。任务要为了实现 `GOAL.md` 中的目标而制定。任务不能过度复杂，如果某个任务过于宏大，可以考虑放到 `GOAL.md` 中。
> 3. 每一条需求，在实现时，在 docs 中创建一个 `yyyymmdd-hhmm-title.md` 的文件。编写开发计划，记录开发结果。
> 4. 开发完成后将上面的文件移动到 `done` 中。保持本文件处于一个简洁的状态。如果这项任务完成了一个值得用户关心的产品亮点，将其更新到根目录下的 `README.md` 中。每次修改提交一个 git commit。
>

- PanelToneStrip 加「✍️ 写 transient_note」按钮 + 弹小输入框 + 时长 chip（15m/30m/1h/2h）：当前只显示不可写，从 UI 写一条临时指令给宠物。
- TG bot 加 `/transient <text> [minutes]` 命令：手机端一键设 transient_note 给宠物（与现 /note 写 general memory 区分，不存盘只挂当前会话）。
- detail.md 编辑器 ⌘K 链接快速插入 popover：有选区当 label 只输 url；无选区弹 url + label 双框 — 键盘党比当前 🔗 按钮快。
- PanelTasks due chip hover tooltip 显示精确倒计时（"还有 47 分钟" / "已逾期 3 小时"）：当前只显日期，glance 不出紧迫度。
- PanelTasks 加「💤 全选 P0-P3 进 multi-select」chip：低优批量管理（cancel / 改 pri / 加 tag），与「☑️ 全选 P7+」对偶。
- PanelMemory butler_tasks 段加「⏸ 全部 silent 1h」批量按钮：临时静音不关全局 — 开会 / 集中写 1 小时用。

















