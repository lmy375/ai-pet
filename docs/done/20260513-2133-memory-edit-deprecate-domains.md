# memory_edit 工具描述 deprecate butler_tasks / todo

## 背景

SQLite GOAL 最后一条：「LLM 通过专用工具读写各域，不再共用 memory_edit」。

- v11 加了 `butler_task_edit` / `todo_edit` 专用工具
- v12 更新了 prompt 引导（proactive + chat 注入）

但 **`memory_edit` tool 自身的 description** 仍写：

> Categories: ai_insights, user_profile, todo, butler_tasks, general
>
> Use `butler_tasks` whenever the owner asks you to DO something on their behalf...

模型一看 description 就以为 butler_tasks / todo 是正常类目 → 与 v12 prompt 引导矛盾。

DedicatedToolStats 显示的占比上不去就是这个原因 —— 工具自描述还在 sell legacy 路径。

## 改动

`src-tauri/src/tools/memory_tools.rs::MemoryEditTool::definition()`：description 全文重写：

```
Create, update, or delete a **memory** item — long-term facts about the
owner, your own thinking notes, generic knowledge worth keeping.

Use this tool ONLY for these categories:
  - user_profile — stable facts about the owner
  - ai_insights — your own thinking / persona_summary / daily_plan / daily_review_<date>
  - general — anything else that doesn't fit a more specific tool

Do NOT use memory_edit for:
  - butler tasks (work the owner delegates to you) → use butler_task_edit
  - reminders (clock-driven nudges) → use todo_edit

If you accidentally call memory_edit with category=butler_tasks or todo it
currently still works as a legacy fallback, but the dedicated tools have
richer per-domain hints and are the correct interface. Use them.
```

`category` enum 不变（仍接受 5 个 category 含 butler_tasks/todo），让 fallback 路径继续 work。Hardening（彻底撤 enum）留给后续 v13+ —— 等 DedicatedToolStats 显示占比稳定到 90%+ 再做。

## 验收

- `cargo build --release` ✅
- `cargo test --lib` ✅ 全 885 通过
- LLM 下一轮 fetch tool definitions 时拿到新描述
- PanelDebug 「专用工具占比」chip 应在几轮调用后看到占比上涨

## 完成

- [x] memory_edit description 重写
- [x] TODO.md 移除该条
- [x] 移到 docs/done/
