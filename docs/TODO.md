> 该文件是产品需求池，描述待开发的所有产品需求。AI 可通过自己对产品需求的分析、代码的分析提出新的产品需求(包括功能的实现、Bug 的修复、代码的重构等),之后进入开发流水线,开发完成后从该文件中移除。
> 1. 如果需求列表已空，则自主开始需求分析,代码分析，提出新的需求，每个需求就不超过 100 个字描述。每个需求一行。
>   - 不要云同步等需求。
>   - 不要专注模式相关的需求。
>   - 不要周报日报相关的需求。
> 2. 每个需求要是一个具体、有价值的、工作量适中的任务。任务要为了实现 `GOAL.md` 中的目标而制定。任务不能过度复杂，如果某个任务过于宏大，可以考虑放到 `GOAL.md` 中。
> 3. 每一条需求，在实现时，在 docs 中创建一个 `yyyymmdd-hhmm-title.md` 的文件。编写开发计划，记录开发结果。
> 4. 开发完成后将上面的文件移动到 `done` 中。保持本文件处于一个简洁的状态。如果这项任务完成了一个值得用户关心的产品亮点，将其更新到根目录下的 `README.md` 中。每次修改提交一个 git commit。
>

- TG bot 加 `/promote <title>` 命令：priority +1（clamp 9）— 比 /pri 更直觉，一步不必算具体 P 值。
- TG bot 加 `/demote <title>` 命令：priority -1（clamp 0）— 与 /promote 对偶，"这个不那么急"一键降。
- PanelDebug 加「📋 复制 logs 路径」chip：~/.config/pet/logs 绝对路径到剪贴板，方便粘到 Finder / VSCode 打开。
- PanelTasks 加「☑️ 全选 P7+ 进 multi-select」chip：精准选高优 pending 进批量模式，与 ⌘A 全选 / 🎯 P7+ filter 互补。
- detail.md 编辑器加 ⌘P toggle preview-only 模式：VS Code preview-only lock 风，看长 detail.md 时焦点纯阅读。
- PanelMemory item action row 加「📅 created N前」hover chip：让 owner glance 这条 memory 何时建立（与 PanelTasks 行内 N前 chip 对偶）。
















