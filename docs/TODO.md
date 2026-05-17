> 该文件是产品需求池，描述待开发的所有产品需求。AI 可通过自己对产品需求的分析、代码的分析提出新的产品需求(包括功能的实现、Bug 的修复、代码的重构等),之后进入开发流水线,开发完成后从该文件中移除。
> 1. 如果需求列表已空，则自主开始需求分析,代码分析，提出新的需求，每个需求就不超过 100 个字描述。每个需求一行。
>   - 不要云同步等需求。
>   - 不要专注模式相关的需求。
>   - 不要周报日报相关的需求。
> 2. 每个需求要是一个具体、有价值的、工作量适中的任务。任务要为了实现 `GOAL.md` 中的目标而制定。任务不能过度复杂，如果某个任务过于宏大，可以考虑放到 `GOAL.md` 中。
> 3. 每一条需求，在实现时，在 docs 中创建一个 `yyyymmdd-hhmm-title.md` 的文件。编写开发计划，记录开发结果。
> 4. 开发完成后将上面的文件移动到 `done` 中。保持本文件处于一个简洁的状态。如果这项任务完成了一个值得用户关心的产品亮点，将其更新到根目录下的 `README.md` 中。每次修改提交一个 git commit。
>

- TG bot `/touched_yesterday` 命令：列昨日 updated_at 命中 task — 与 /touched_today 同模板（昨日复盘 audit）。
- TG bot `/oldest_done [N]` 命令：列最早完成的 N 条 done task（按 updated_at asc）— /recent done 反向，看「这条做了多久」。
- PanelTasks 行 hover 「💤 N 分后醒」chip：snoozed task 显距 wake 倒计时 — 既有 [snooze: HH:MM] marker parse + 倒计。
- TG bot `/move_to <title> <category>` 命令：跨 memory category 迁移 — 复用既有 `memory_move_category` Tauri 命令。
- ChatMini ambient row 「📊 今日 N 消息」chip：显当前 session 今日 user+assistant 消息总数 — 活跃度信号。
- TG bot `/cascade_rename <old> :: <new>` 命令：rename + 自动追踪 detail.md / [blockedBy:] 内 `「<old>」` ref 同步替换 — 与 /edit_title 自动维护选择。



































