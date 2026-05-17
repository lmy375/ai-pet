# PanelTasks task ctx menu「🪞 克隆任务」+ task_clone 后端（iter #281）

## Background

owner 复用任务模板的需求：把"每月报销整理"模板克隆一份 schedule 略调 →
新任务；或 "上次 standup 流程"克隆作"今天 standup"。当前路径：编辑 → 选
中描述 → ⌘A+⌘C → 新建 → 粘贴 → 重起 title。多步且容易漏 markers / detail。

本迭代加 task_clone 后端 + PanelTasks 右键「🪞 克隆任务」一键命令：
- title 自动 `${源} (副本)`（重名累加 (副本 2) ... (副本 9)）
- description strip 终态 / snooze marker，保留 header / schedule / tag /
  pinned / silent / blockedBy / reminderMin
- detail.md 内容一并拷贝

## Changes

后端：

- `src-tauri/src/task_queue.rs`：新增 `strip_for_clone(description)` pure
  helper（strip `[done]` / `[result]` / `[error]` / `[cancelled]` /
  `[archived]` / `[snooze:]`）+ 4 个单测：剥终态 + snooze marker / 保留
  owner-intent markers / 干净 pending 幂等 / 剥 error reason 让 clone fresh

- `src-tauri/src/commands/task.rs`：新增 `task_clone` tauri 命令 +
  `task_clone_inner`：
  - 校验 title 非空 / 找得到
  - 探 title 唯一性：`(副本)` → `(副本 2)` → ... → `(副本 9)` 都占用时
    Err 让 owner 先 rename 旧的（避免 (副本 100) 这种丑陋积累）
  - 读源 detail.md 全文（IO 失败兜空字符串）
  - strip_for_clone 后 memory_edit("create", ...)
  - decision_log push "TaskClone"
  - 返新 title 让前端 toast 显

- `src-tauri/src/lib.rs`：注册 `task_clone` 命令。

前端：

- `src/components/panel/PanelTasks.tsx`：在右键 ctx 菜单的「📑 复制为
  Markdown」之后插「🪞 克隆任务」按钮：
  - click → invoke + reload + 4s toast `🪞 已克隆为「<新 title>」`
  - 失败显错误 toast

## Key design decisions

- **strip_for_clone 而非复用 strip_archive_markers**：archive unarchive 路径
  保 backward compat（不剥 [snooze:]），但 clone 语义更强 — owner 想要
  fresh task spec，defer 状态不应跟随。新 helper 不影响既有 unarchive。
- **(副本) suffix + 编号上限 9**：到 (副本 9) 仍占用时 Err 让 owner 先重
  命名，防 (副本 100) 这种污染。owner 实际场景里克隆 2-3 次足够；超过 9
  是滥用信号。
- **保留 schedule 等 owner-intent**：clone 出来的 task 应继承"执行 spec"
  （[every: ...] / tag / pinned / blockedBy / reminderMin），让 owner 不
  必重新设置。终态 / snooze 是状态历史不属于 spec。
- **复用 memory_edit("create", ...)**：避免重写 file 创建路径；既有重名
  防御 + detail.md 写盘 + SQL mirror 都自动走对路径。
- **decision_log "TaskClone"**：让 PanelDebug 决策日志能 audit "owner
  克隆了哪条"，与 TaskRetry / TaskMarkDone / TaskUndoDone / TaskSkipOnce
  / TaskClone 同 audit 维度。

## Verification

- `cargo check` ✅
- `cargo test`（含 4 新 strip_for_clone 测试 + 全表 1077 通过）✅
- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.24s)
