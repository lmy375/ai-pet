> 该文件是产品需求池，描述待开发的所有产品需求。AI 可通过自己对产品需求的分析、代码的分析提出新的产品需求(包括功能的实现、Bug 的修复、代码的重构等),之后进入开发流水线,开发完成后从该文件中移除。
> 1. 如果需求列表已空，则自主开始需求分析,代码分析，提出新的需求，每个需求就不超过 100 个字描述。每个需求一行。
>   - 不要云同步等需求。
>   - 不要专注模式相关的需求。
>   - 不要周报日报相关的需求。
> 2. 每个需求要是一个具体、有价值的、工作量适中的任务。任务要为了实现 `GOAL.md` 中的目标而制定。任务不能过度复杂，如果某个任务过于宏大，可以考虑放到 `GOAL.md` 中。
> 3. 每一条需求，在实现时，在 docs 中创建一个 `yyyymmdd-hhmm-title.md` 的文件。编写开发计划，记录开发结果。
> 4. 开发完成后将上面的文件移动到 `done` 中。保持本文件处于一个简洁的状态。如果这项任务完成了一个值得用户关心的产品亮点，将其更新到根目录下的 `README.md` 中。每次修改提交一个 git commit。
>

- PanelMemory item description 输 `#` 时弹既有 tag 补全 popover：与 iter #390 PanelTasks 搜索框对偶 — 让 owner 在写 memory 时也享受 tag 自动补全免敲错。
- PanelTasks 加「📊 priority distribution」mini sparkline chip：一行显 P0-P9 各档 pending 数 mini bar，让 owner 一眼看 priority 分布偏态。
- TG bot 加 `/edit_due <title> <preset>` 命令：preset 接 tomorrow/tonight/今晚/next_monday/+30m 等友好词 — 手机端免手敲 ISO 日期改 due。
- ChatMini ambient hint 行 chip click → 跳 PanelDebug 对应卡片：iter #383 hint 只显数字，click 应 deep-link 到详情（开 panel + 切对应 tab + 滚到位）。
- pet ctx menu 加「🔄 重启 LLM 连接」：调试 LLM 卡死场景一键 reset chat HTTP client（与既有 reconnect_mcp 同模式但针对 chat backend）。






















