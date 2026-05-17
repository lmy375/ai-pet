> 该文件是产品需求池，描述待开发的所有产品需求。AI 可通过自己对产品需求的分析、代码的分析提出新的产品需求(包括功能的实现、Bug 的修复、代码的重构等),之后进入开发流水线,开发完成后从该文件中移除。
> 1. 如果需求列表已空，则自主开始需求分析,代码分析，提出新的需求，每个需求就不超过 100 个字描述。每个需求一行。
>   - 不要云同步等需求。
>   - 不要专注模式相关的需求。
>   - 不要周报日报相关的需求。
> 2. 每个需求要是一个具体、有价值的、工作量适中的任务。任务要为了实现 `GOAL.md` 中的目标而制定。任务不能过度复杂，如果某个任务过于宏大，可以考虑放到 `GOAL.md` 中。
> 3. 每一条需求，在实现时，在 docs 中创建一个 `yyyymmdd-hhmm-title.md` 的文件。编写开发计划，记录开发结果。
> 4. 开发完成后将上面的文件移动到 `done` 中。保持本文件处于一个简洁的状态。如果这项任务完成了一个值得用户关心的产品亮点，将其更新到根目录下的 `README.md` 中。每次修改提交一个 git commit。
>

- detail.md 编辑器加 ⌘B 加粗 / ⌘I 斜体快捷键：选区周围加 `**...**` / `*...*`，markdown 编辑器现代基础功能。
- TG bot 加 `/feedback <text>` 命令：owner 给 pet 反馈写 feedback_history（喜欢 / 不喜欢的行为），影响后续 proactive cycle。
- TG bot 加 `/cancel-all-error confirm` 命令：批量 cancel 所有 error 状态任务，必须带 confirm token 防误触。
- PanelChat 加 ⌘F 行内搜索 messages：长 chat 历史 keyword 命中跳转，长会话查找用。
- PanelMemory item action row 加 ✏️ rename mini-input：与既有双击 title rename 对偶 — 鼠标党直接按按钮免双击。
- detail editor 工具栏加「🔗 插 link 模板」按钮：选区 → `[text](url)` wrap，光标落 url 处让 owner 填。
















