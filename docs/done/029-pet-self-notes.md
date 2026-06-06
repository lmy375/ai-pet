# 029 · pet self_note — 自我进化的 pet 侧内心维度

到目前 mood / memory / 沟通偏好 (019) / topic arc (027) 全部是 user-driven 维度。pet 自己「写日记」反思的纯内心从未存在。GOAL「自我进化」要求 pet 有独立情绪 / 记忆 / 技能 — 缺一条"pet 怎么看待自己"的内心通道。落地后 pet 偶尔在 proactive utterance 中引用，正是 user 真正感到 pet 有内心的瞬间。

需求：
- PanelMemory 新增固定 cat `self_note`，user 可读不可写，pet-owned。
- 触发写入事件：daily_review 完成后 / mood_history 跨度变化超阈值 / 019 收到严厉沟通反馈 / 020 chain 完成 / 012 deferred 完成。每事件至多写 1 条，自然语言、第一人称。
- 内容含 ts + 触发事件 ref（哪个 chain / 哪条 feedback）+ pet 一句心境自述。
- 每周一次 self-review：pet 读最近 7d self_note，提炼一句"我最近发现自己 X / 我有点担心 Y / 我挺喜欢这样 Z"作为偶发 proactive utterance 推给 user（鉴 026 stress / 017 mood gate）。
- TG `/self_notes [N]` user 可读最近 N 条；`/self_notes_pause` 让 pet 停止写入（尊重 user 觉得多余的边界），`/self_notes_resume` 反向。
- 023 session_distill / 024 forget / 025 consolidate 通路不触碰 self_note cat（pet-owned 隔离）。

---
实现笔记：
- MemoryIndex Default 加固定 cat `self_note`（label「宠物内心」）；既有 PanelMemory 读路径自动渲染。
- 新建 `src-tauri/src/self_note.rs`：`TriggerKind` enum 5 变体（DailyReview / DeferredDone / CommPrefAdd 已通；ChainCompleted / MoodSwing 占位待补 hook）；`spawn_record(kind, ref_id)` fire-and-forget 调 memory_edit 落 description 协议 `[self_note: YYYY-MM-DD] [trigger: kind:ref] <第一人称模板文案>`。Pause 状态 `self_note_paused.json`；Tauri 命令 `self_note_list` / `self_note_set_paused` / `self_note_is_paused`。4 单测覆盖 parse / template / label 协议稳定性。
- 触发 hook（3 处接通）：proactive.rs `maybe_run_daily_review` 末尾 / `maybe_run_deferred_task` final_status==Done / `communication_prefs::add_preference` 末尾。每条 fire-and-forget，pause 时 no-op。
- 023/024/025 通路 cat-隔离已自然满足：024 forget_propose / 025 consolidate_propose 显式 `categories_to_scan = ["ai_insights", "user_profile"]`；023 session_distill prompt 教 LLM 写 `ai_insights` / `user_profile`（软约束 — 后续可强化）。
- TG `/self_notes [N]` / `/self_notes_pause` / `/self_notes_resume` 三件套 wired。
- **缺口**：（1）LLM-driven 一句心境自述未做 —— v1 用 TriggerKind::template_text 静态模板；每事件 LLM call 成本未知 + 异步落库困难，留 follow-up。（2）每周 self-review proactive utterance 未做。（3）ChainCompleted / MoodSwing 触发 hook 未接（变体已占位）。（4）「user 可读不可写」前端 UI 隔离未做 —— 后端 memory_edit 仍允许写入（spawn_record 也通过它落库），靠语义边界约束 + PanelMemory 没有 UI 入口选 self_note cat 来实际隔离。
