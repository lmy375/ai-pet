# 020 · task 依赖链 — "做完 X 再做 Y"

reminder / 012 deferred / 011 scheduled 都是独立 task。但 user 自然语言常给出依赖关系：「今晚 9 点提醒我看球，看完球别忘了打电话给爸妈」「文档写完帮我跑一下抓取」「会议结束后整理今天的笔记」。当前要么落两条独立 reminder（依赖丢失），要么只落第一条（第二条遗忘）。

需求：
- LLM 在 task 解析阶段识别依赖结构（"X 之后 / X 完了 / 完成 X 后 / 等 X 结束"），落地为 task chain：chain[0] 是 trigger，chain[1..] 在前一个 marker 完成后才可被 fire。
- chain 节点支持混合类型：节点可为 reminder / deferred / scheduled 任一，依赖关系跨类型生效（reminder 完成可触发 deferred fire）。
- 完成 marker 来源：reminder 到点 user 标 done / user 对话回复完成（"看完了 / 写完了"）/ deferred_task fire 自身完成。
- 14d 内 chain[0] 未完成则整条 chain 标 stale，移入 014 PanelReports 归档 chip（不丢失，但不再 fire 后续）。
- PanelReports 与 PanelTasks 中 chain 显示为可折叠 group，单条 entry 标记 `chain N/M` 进度 chip。
- TG `/chain` 查看当前所有未完成 chain 状态；不引入新创建命令（创建路径就是自然语言）。

---
实现笔记（MVP 范围）：
- `DeferredTask` 加 `depends_on: Option<String>` 字段（serde default None 向前兼容）。`add_task(spec, depends_on)` 新签名 + dangling 引用守门（depends_on 指向不存在的 id 直接 Err）。`pick_oldest_pending` 升级为 runnable-aware：跳过 `depends_on != None && upstream.status != Done` 的条目（也含 Failed/Cancelled/missing 三种 unmet 形态）。
- 14d stale：`stale_chain_roots(store, now)` 找超 14d 的 Pending root；`descendants_of(store, root_id)` BFS 拍后代列表；`mark_stale_chains()` 一次扫 + 标，把整链 Pending 全设 Failed + finished_at。`maybe_run_deferred_task` 进 mood gate 前调一次。
- LLM tool：`defer_task` 加 `depends_on` 参数；description 教 LLM 何时拼链路、何时单独。
- TG `/chain` 命令：按 chain root 分组，每 root 出 "进度 X/Y" header + 每节点 status icon + spec excerpt。stale chain 已被 mark_stale_chains 标 Failed 故不出现在本视图（落 PanelReports archived chip）。
- **重大 gap**（未做，需后续 iter）：
  - **跨类型 chain**（reminder / scheduled → deferred）未支持。reminder 完成 → unlock deferred / scheduled 完成 → unlock deferred 都没钩子；只 deferred → deferred 一条 path 跑通。
  - **PanelReports / PanelTasks "chain N/M" chip + 可折叠 group**：前端 UI 未做（与既有 butler_subtask chip 同类的工作量，本轮没碰 PanelMemory/PanelTasks）。TG /chain 是唯一可视化入口。
  - **自然语言完成识别**（user 说「写完了」自动 mark Done）未做；现在仍需 LLM 显式调 mark_finished tool（v1 该 tool 暂未提供，依赖系统 fire 时的 done 路径）。
