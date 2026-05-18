> 该文件是产品需求池，描述待开发的所有产品需求。AI 可通过自己对产品需求的分析、代码的分析提出新的产品需求(包括功能的实现、Bug 的修复、代码的重构等),之后进入开发流水线,开发完成后从该文件中移除。
> 1. 如果需求列表已空，则自主开始需求分析,代码分析，提出新的需求，每个需求就不超过 100 个字描述。每个需求一行。
>   - 不要云同步等需求。
>   - 不要专注模式相关的需求。
>   - 不要周报日报相关的需求。
> 2. 每个需求要是一个具体、有价值的、工作量适中的任务。任务要为了实现 `GOAL.md` 中的目标而制定。任务不能过度复杂，如果某个任务过于宏大，可以考虑放到 `GOAL.md` 中。
> 3. 每一条需求，在实现时，在 docs 中创建一个 `yyyymmdd-hhmm-title.md` 的文件。编写开发计划，记录开发结果。
> 4. 开发完成后将上面的文件移动到 `done` 中。保持本文件处于一个简洁的状态。如果这项任务完成了一个值得用户关心的产品亮点，将其更新到根目录下的 `README.md` 中。每次修改提交一个 git commit。
>

- PanelTasks toolbar 加「仅显 💤 7d+ idle」filter toggle — 一键聚焦 stale backlog（与 hover chip 呼应）。
- TG `/cat_decay_7d`：列 7d updated_at 都没动的 cat — stale cat detection，与 /cat_growth_7d 反向。
- PanelMemory item hover「📊 7d updates」chip：扫本 item updated_at 算近 7 天有几次 update — 单 item 活跃度。
- ChatMini bubble hover「⏱ N 分前」chip：与 bubble ts 互补，显距 now 相对时间（精度比 ts 高 — ambient awareness）。
- TG `/aliases <title>`：列本 task description 历史 rename 痕迹（butler_history 扫 create+rename event）— 「这条曾叫什么」audit。











































