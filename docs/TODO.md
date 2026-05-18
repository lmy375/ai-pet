> 该文件是产品需求池，描述待开发的所有产品需求。AI 可通过自己对产品需求的分析、代码的分析提出新的产品需求(包括功能的实现、Bug 的修复、代码的重构等),之后进入开发流水线,开发完成后从该文件中移除。
> 1. 如果需求列表已空，则自主开始需求分析,代码分析，提出新的需求，每个需求就不超过 100 个字描述。每个需求一行。
>   - 不要云同步等需求。
>   - 不要专注模式相关的需求。
>   - 不要周报日报相关的需求。
> 2. 每个需求要是一个具体、有价值的、工作量适中的任务。任务要为了实现 `GOAL.md` 中的目标而制定。任务不能过度复杂，如果某个任务过于宏大，可以考虑放到 `GOAL.md` 中。
> 3. 每一条需求，在实现时，在 docs 中创建一个 `yyyymmdd-hhmm-title.md` 的文件。编写开发计划，记录开发结果。
> 4. 开发完成后将上面的文件移动到 `done` 中。保持本文件处于一个简洁的状态。如果这项任务完成了一个值得用户关心的产品亮点，将其更新到根目录下的 `README.md` 中。每次修改提交一个 git commit。
>

- Backend lift：`memory_rename` 加 `record_event("rename", new_title, "[was: <old>]")` 调用 — 让未来 rename 在 butler_history 留痕。
- TG `/idle_7d`：列 pending + updated_at ≥ 7d 前的 task — PanelTasks 💤 filter 的 TG 端对偶。
- PanelMemory item 「🔥 24h fresh」visual badge：item.updated_at 在 24h 内时浮 chip — 直接信号无需 hover。
- PanelTasks chip-bar「🚀 今日 active P7+」chip：created_at 今日 + priority ≥ 7 的 task 数 — 高优 sprint 起步信号。
- TG `/cat_decay_30d`：/cat_decay_7d 30 天 cousin — 长周期 zombie cat 检测（区分「停滞 1 周」vs「停滞 1 月」严重度）。
- ChatMini bubble role 切换 hover hint：hover assistant bubble 时显「↺ 重发」/ user bubble 显「✏️ 编辑后重发」入口提示（仅提示，不改 action）。












































