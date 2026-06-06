# 010 · butler_task 拆解 propose — 管家从"记一条"到"帮你想下一步"

butler_task 系统（b6a193e）目前接收单条 desc + 30d archive 节奏，但用户给 vague 任务（"周末整理书房"、"准备月底汇报"、"准备面试"）时宠物只能原文记一条，毫无 butler 味。GOAL「自我进化 → 宠物管家 / task-execution」需要在这一刻发挥。

需求：
- 在现有 butler_task add 路径之后挂一个 follow-up 钩子：desc 命中"vague"特征（长度 ≤ 20 字 + 含"准备 / 整理 / 规划 / 学习 / 复习 / 安排"等动词）即触发。
- 触发后宠物在 ChatMini / TG（取决于来源端）追加一句反问，给出 3-5 个候选 subtask（LLM 围绕 desc 生成），形式："我把它拆这样，你看？"。
- 用户回复确认（"好 / 添加 / 都要"）→ 候选 subtask 落 PanelTasks 作为 child，并写入 parent butler_task 的 `subtasks` 字段；回复编辑后列表则照编辑落。
- 用户跳过 / 拒绝 / 不回复 → parent butler_task 保留原状，不补、不再追问同一 parent。
- subtask 命中归档/完成时正常走现有 butler_task 流程；parent 显示「N/M」进度 chip。
- vague 触发阈值（动词集、长度）做常量集中，不暴露给用户。

---
实现笔记：
- 新建 `src-tauri/src/subtask_helpers.rs`：`VAGUE_VERBS`/`VAGUE_DESC_MAX_CHARS=20` 常量；`is_vague_butler_desc` / `parse_parent_prefix` / `compute_subtask_progress` 三纯函数。`inject_subtask_propose_layer` 把「vague butler_task 拆解 + 用 `[parent: <title>]` 前缀」rule 注入 system 上下文，chat.rs + bot.rs::run_chat_turn 同时挂入。Tauri 命令 `butler_subtask_progress(parent_title)` 给前端读 (done, total)。
- 不改 SQLite schema：parent↔child 关联走 description 内 `[parent: <parent_title>]` 文本协议，LLM 通过既有 `butler_task_edit` 工具创建 child butler_tasks（与普通 task 形态一致 —— done / archive / consolidate 流程不变；归档时 child 单独 archive，parent 自己另算）。
- UX 闭环：与 GOAL 004 同款 LLM-mediated 路径 —— 注入层让 LLM「同回合」识别 vague 并主动提议；用户口头同意时 LLM 自己调 butler_task_edit 批量落盘。不在 Rust 端做 NL accept-parse。
- 前端 N/M chip：PanelMemory butler_tasks 行新增 📋 chip，扫 cat.items 找 `[parent: <my_title>]` 命中 children 计 done/total，0 子时不渲。PanelTasks（runtime queue view）未改 —— GOAL 「parent 显示 N/M」字面落在 PanelMemory（butler_tasks 真正落点），PanelTasks 是另一个抽象层（runtime dispatch），本轮不动避免越界。
- LLM-side 软约束局限：vague 识别 / subtask 质量 / 用户同意时是否真的 batch-create —— 都靠 LLM compliance。若实地观察到 LLM 不照办，可补 Rust-side hook：butler_task_edit_impl 里 create 完调 `is_vague_butler_desc` log 一条 "[vague-create]" event，下轮 chat 注入再带 reminder。本轮先信任 prompt。
