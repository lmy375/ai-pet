> 该文件是产品需求池，描述待开发的所有产品需求。AI 可通过自己对产品需求的分析、代码的分析提出新的产品需求(包括功能的实现、Bug 的修复、代码的重构等),之后进入开发流水线,开发完成后从该文件中移除。
> 1. 如果需求列表已空，则自主开始需求分析,代码分析，提出新的需求，每个需求就不超过 100 个字描述。每个需求一行。
>   - 不要云同步等需求。
>   - 不要专注模式相关的需求。
>   - 不要周报日报相关的需求。
> 2. 每个需求要是一个具体、有价值的、工作量适中的任务。任务要为了实现 `GOAL.md` 中的目标而制定。任务不能过度复杂，如果某个任务过于宏大，可以考虑放到 `GOAL.md` 中。
> 3. 每一条需求，在实现时，在 docs 中创建一个 `yyyymmdd-hhmm-title.md` 的文件。编写开发计划，记录开发结果。
> 4. 开发完成后将上面的文件移动到 `done` 中。保持本文件处于一个简洁的状态。如果这项任务完成了一个值得用户关心的产品亮点，将其更新到根目录下的 `README.md` 中。每次修改提交一个 git commit。
>

- PanelDebug 加「📊 近 1h tokens」chip：扫 llm_logs 算近 1 小时 token 累计 — audit 高峰耗用入口。
- detail.md 编辑器「⌘⇧C 复制当前段」shortcut：复制光标所在 markdown heading 段（heading 到下一 heading 之间）。
- TG bot `/sleep_until <HH:MM>` 命令：mute 到指定时刻（与 /mute N 互补 — 「安静到 8 点」更自然）。
- PanelTasks 行右键加「📌+⏰5min 提醒」组合项：钉住 + 5 分钟后软提醒 — sprint 突发场景。
- ChatMini 顶部加「⏱ pet 沉默 N 分」chip：自上次 pet 主动开口算起 — owner 觉察 pet 卡住入口。
- PanelMemory 段标题加「🔀 按 created 排」toggle：默认 updated 倒序；切到 created 倒序 — 「我什么顺序加的」audit。


























