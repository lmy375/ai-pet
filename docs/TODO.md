> 该文件是产品需求池，描述待开发的所有产品需求。AI 可通过自己对产品需求的分析、代码的分析提出新的产品需求(包括功能的实现、Bug 的修复、代码的重构等),之后进入开发流水线,开发完成后从该文件中移除。
> 1. 如果需求列表已空，则自主开始需求分析,代码分析，提出新的需求，每个需求就不超过 100 个字描述。每个需求一行。
>   - 不要云同步等需求。
>   - 不要专注模式相关的需求。
>   - 不要周报日报相关的需求。
> 2. 每个需求要是一个具体、有价值的、工作量适中的任务。任务要为了实现 `GOAL.md` 中的目标而制定。任务不能过度复杂，如果某个任务过于宏大，可以考虑放到 `GOAL.md` 中。
> 3. 每一条需求，在实现时，在 docs 中创建一个 `yyyymmdd-hhmm-title.md` 的文件。编写开发计划，记录开发结果。
> 4. 开发完成后将上面的文件移动到 `done` 中。保持本文件处于一个简洁的状态。如果这项任务完成了一个值得用户关心的产品亮点，将其更新到根目录下的 `README.md` 中。每次修改提交一个 git commit。
>

- PanelTasks 行加「📜 复制 raw_description」hover chip：含全 markers 的 raw 一键复制 — debug / dup task 场景。
- PanelMemory 加全局「📌 pinned-only」toggle：与既有 sort toggle 同行；显所有 cat 仅 pinned items — 总览「我钉了哪些」。
- TG bot `/peek <title>` 命令：显 task 一行紧凑视图（status + due + 关键 markers）— 比 /show 紧凑。
- detail.md 编辑器「⌘P heading palette」shortcut：弹 fuzzy palette 列 detail.md heading，select 跳位置 — 长 doc 导航。
- ChatPanel 输入框加「📜 复制全 input」按钮：清空前一键复制当前 input — 防误删长输入。































