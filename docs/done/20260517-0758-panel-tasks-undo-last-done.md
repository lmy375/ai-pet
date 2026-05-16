# PanelTasks「🔁 撤销最后一条 done」按钮 + task_undo_done 后端（iter #267）

## Background

误标 done 是 owner 高频 footgun — 键盘 `d` 一按 / 行内 ✓ 一点就标了。当前撤
销路径要走"右键 → 取消 → 输理由 → 提交"四步，且 cancelled 是终态不是
pending，恢复语义不对。

本迭代加专用「🔁 撤销最后一条 done」按钮：armed 二次确认后调新后端命令
`task_undo_done`，剥 description 里 `[done]` / `[result: ...]` marker 回
pending（与 task_mark_done 对偶）。owner-intent markers（schedule / tag /
pinned / silent / snooze / blockedBy / detail.md）全保留。

## Changes

- `src-tauri/src/task_queue.rs`：
  - 新增 `strip_done_markers(description)` pure helper：用
    `remove_bracketed_segments` 剥 `[done` / `[result` 两类前缀的 bracketed
    段，保留所有其它 markers。
  - 4 个新单测：清 done + result / 保留 owner-intent markers / 干净 pending
    幂等 / 多 result 段都剥

- `src-tauri/src/commands/task.rs`：
  - 新增 `task_undo_done` tauri 命令 + `task_undo_done_inner` 复用核心
  - 校验：title 空 → Err；找不到 → Err；status != Done → Err（让前端给明确
    反馈）
  - memory_edit("update", ...) 写回 stripped description；不重写 detail.md
    （与 task_retry 同保留语义 — LLM 在 done 时可能写过的 progress 笔记保留）
  - decision_log push "TaskUndoDone"

- `src-tauri/src/lib.rs`：注册 `task_undo_done` 命令。

- `src/components/panel/PanelTasks.tsx`：
  - 新增 `undoLastDoneArmed: boolean` state + `undoLastDoneTimerRef` 5s 还原
    + `undoLastDoneBusyRef` 防双触
  - 新增 `handleUndoLastDone` useCallback：从 `completionStats.weekList[0]`
    （已按 updated_at desc 排）取最后一条 done；空 → bulkResultMsg 提示；
    armed 流程 → invoke `task_undo_done` → reload + 成功 toast
  - 在 ✅ 今日完成 chip + 7-day sparkline 之后插「🔁 撤销 done「X」」按钮
    （仅 completionStats.week > 0 时显），armed 状态变红字 + 露具体标题（截
    12 字）

## Key design decisions

- **专用命令而非复用 task_retry**：retry 仅允许 Error→Pending；done→pending 是
  不同语义事件（误操作撤销 vs 重试失败），独立命令保 decision_log 区分。
- **strip_done_markers 仅剥 done / result**：保留 owner-intent markers（schedule
  / tag / pinned / silent / snooze / blockedBy）让任务回 pending 后上下文完整。
  与 `strip_archive_markers` 的"恢复归档"更激进策略不同。
- **armed 二次确认显具体标题**：armed 文案显"⟲「X」"让 owner 知道会撤哪条
  （而非泛泛的 "撤销 done"），防误操作撤错 task。
- **取最后一条 done = weekList[0]**：weekList 已按 updated_at desc 排；
  weekList[0] 就是最近完成的那条。即使是几天前完成的也允许撤销 —— 误标可
  能延迟发现。
- **完整 toast 反馈 ✓ + 失败 actionErr**：成功 4s 自清 toast；失败保留
  actionErr 直到下次操作（与既有失败反馈风格一致）。

## Verification

- `cargo check` ✅
- `cargo test`（含 4 新 strip_done 测试 + 全表 1057 通过）✅
- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.25s)
