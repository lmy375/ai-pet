> 该文件是产品需求池，描述待开发的所有产品需求。AI 可通过自己对产品需求的分析、代码的分析提出新的产品需求(包括功能的实现、Bug 的修复、代码的重构等),之后进入开发流水线,开发完成后从该文件中移除。
> 1. 如果需求列表已空，则自主开始需求分析,代码分析，提出新的需求，每个需求就不超过 100 个字描述。每个需求一行。
>   - 不要云同步等需求。
>   - 不要专注模式相关的需求。
>   - 不要周报日报相关的需求。
> 2. 每个需求要是一个具体、有价值的、工作量适中的任务。任务要为了实现 `GOAL.md` 中的目标而制定。任务不能过度复杂，如果某个任务过于宏大，可以考虑放到 `GOAL.md` 中。
> 3. 每一条需求，在实现时，在 docs 中创建一个 `yyyymmdd-hhmm-title.md` 的文件。编写开发计划，记录开发结果。
> 4. 开发完成后将上面的文件移动到 `done` 中。保持本文件处于一个简洁的状态。如果这项任务完成了一个值得用户关心的产品亮点，将其更新到根目录下的 `README.md` 中。每次修改提交一个 git commit。
>

- TG bot 加 `/silent_all [minutes]` 命令：手机端批量 silent butler_tasks N 分钟（与 iter #366 桌面按钮对偶 — 但 TG 端要把 frontend timer 改成无状态 / 后端定时撤回）。
- detail.md 编辑器选区行 Tab / Shift+Tab 多行缩进 / 反缩进：markdown 列表层级编辑加速（VSCode / Sublime 通用习惯，与 ⌘B/I 同 IDE-like 集群）。
- PanelTasks multi-select 模式加「📋 复制选中 N 条标题」批量按钮：选完一键拷 title 列到剪贴板（给团队 / 周会清单 / 外部 ticket 导出用）。
- PanelMemory item 加「⏰ 一次性 5/15/30 分钟后提醒」chip：与既有 reminderMin（fire 前 N 分钟）区分 — 一次性 alarm 到点弹 ChatMini 软提醒，不挂 schedule。
- TG bot 加 `/feedback_history [N]` 命令：列最近 N 条 feedback_history.log 条目（owner 回看自己给 pet 留过什么反馈 / 验证生效）。
- PanelDebug 日志 tab 加「🔍 过滤 keyword」substring 实时 filter：长日志（千行）找特定 error / event / task title 用，避免肉眼扫读。


















