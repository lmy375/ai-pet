# PanelMemory butler_tasks「⏭ skip 一次」按钮 + task_skip_once 后端（iter #276）

## Background

owner 偶尔想"今天先不做这条 every 任务"（如 09:00 standup 但今天请假），
但不想改 schedule（明天还要）。当前路径：[snooze: YYYY-MM-DD 09:01] 写到
描述里 — 显得太重 + 要手敲 timestamp，错就漏一天。

本迭代加「⏭ skip」按钮：刷 updated_at 到 now → `isButlerDue` 走 false →
本轮不会被 proactive 选；下一轮仍按原 schedule 触发。与 ▶️ 现在跑 互补：
一个推进，一个推后。

## Changes

后端：

- `src-tauri/src/commands/task.rs`：新增 `task_skip_once` tauri 命令 +
  `task_skip_once_inner`：
  - 校验：title 空 / 找不到 / done / cancelled 拒绝（error 不拒 — owner 可
    能在 LLM 报错后"先放一会儿再说"）
  - 实现：`memory_edit("update", "butler_tasks", title, Some(item.description),
    None)` 原样 re-write，memory_edit 自动 stamp updated_at 到 now
  - `decision_log.push("TaskSkipOnce", title)` 给 audit

- `src-tauri/src/lib.rs`：注册命令。

前端：

- `src/components/panel/PanelMemory.tsx`：
  - state：`skipOnceArmedTitle` + `skipOnceArmedTimerRef` 3s 还原 +
    `skipOnceBusyTitle` 防双触
  - `handleSkipOnce(title)`：armed → 二次点 invoke + reload + 4s 成功 toast
  - 按钮 render：在 ▶️ 现在跑 之后插「⏭ skip」（仅 butler_tasks +
    `due === true` 时显）。armed 状态 amber tint（vs ▶️ 现在跑的 red），让两
    个反向动作色感不混。

## Key design decisions

- **重 write 同 description 而非新 markers**：memory_edit 已实现 stamp
  updated_at — 复用最便宜路径。不污染 description 历史（不像 [snooze:]
  会留下 marker 痕迹）。
- **仅 due 时显按钮**：未 due 的 task skip 是 no-op + 让 owner 困惑"我点了
  但没变化"。due gate 让按钮的可见性 = 操作的可行性。
- **armed amber tint 与现跑 red 区分**：颜色暗示语义 — ▶️ 现在跑（red =
  紧急 / 立即触发）vs ⏭ skip（amber = 推后 / 缓冲）。
- **error 状态允许 skip**：error 是 LLM 试过但失败；owner 可能想"先跳本轮
  待会儿再 retry"。done / cancelled 终态拒绝（skipping done 无意义）。
- **decision_log "TaskSkipOnce"**：让 PanelDebug 决策日志能看到"owner 主动
  跳了某条"，与 TaskRetry / TaskMarkDone / TaskUndoDone 同 audit 维度。

## Verification

- `cargo check` ✅
- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.24s)
