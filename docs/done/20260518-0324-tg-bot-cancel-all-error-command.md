# TG bot `/cancel_all_error confirm` 命令（批量清 error）（iter #353）

## Background

owner 在 TG 端有 `/cancel <title>` 单条取消 / `/retry <title>` 单条重试。
但当 error 任务积累多条（e.g., 周末发现 10+ task 都因网络 / 配置失败）
想一次性清空时，逐条 `/cancel` 操作太重。本迭代加 `/cancel_all_error`
批量入口 — 走破坏性操作 confirm token 防误触模式。

## Changes

### `src-tauri/src/telegram/commands.rs`

- enum `CancelAllError { confirmed: bool }` 变体（confirmed 字段由 parser
  根据 trailing token 决定）
- `name()` → `"cancel_all_error"`（**下划线**而非 dash — TG setMyCommands
  要求 name 仅 lowercase ASCII / 数字 / 下划线）
- `title()` 归入无参桶
- 解析器：trailing token 全 trim 后 `eq_ignore_ascii_case("confirm")` 才
  算 confirmed；其它任何 token（"yes" / "ok" / 空）都算 false
- 新 pure formatter `format_cancel_all_error_reply(confirmed,
  error_count_before, cancelled_ok, cancelled_err)`：
  - confirmed=false + 0 errors → "暂无 error 任务"
  - confirmed=false + N errors → "有 N 条 error" + "必须带 `confirm`" +
    显示完整命令文本
  - confirmed=true + 全 0 → "暂无 error 可 cancel ✨"
  - confirmed=true + ok>0 → "已批量 cancel N 条" + 可选 ⚠️ "M 条失败"
    （并发改 status race）+ /tasks / /retry follow-up hint
- registry zh + en 都加 ("cancel_all_error", desc)
- format_help_text 全表加 `/cancel_all_error confirm` 行（/feedback 之后）
- format_help_for_topic 加 "cancel_all_error" key + /cancel / /retry /
  /stats 交叉引用
- ALL_HELP_TOPICS 加 "cancel_all_error"
- 两 drift-defense 名单同步加 "cancel_all_error"

### `src-tauri/src/telegram/bot.rs`

- 加 `TgCommand::CancelAllError { confirmed }` handler arm（在 Feedback
  arm 之前）：
  - `read_tg_chat_task_views(chat_id.0)` 获取 chat-scoped tasks
  - filter `TaskStatus::Error` → error_titles
  - confirmed=false → format usage hint
  - confirmed=true → 取 DecisionLogStore，for each title 调
    `task::task_cancel_inner(title, "", decisions)`，计 ok / err 数；
    失败不阻断后续（log race 容忍）

### Tests（7 个新 unit test）

- parser：
  - 无 token → confirmed=false
  - "yes" 等非 confirm token → confirmed=false（防 owner 误打）
  - "confirm" → true
  - case-insensitive ("CONFIRM" / "Confirm" / "confirm" 同结果)
- formatter：
  - unconfirmed + 0 errors → "暂无 error 任务"
  - unconfirmed + N errors → 显 N 条 + 要求 confirm + 完整命令
  - confirmed + 0 → "暂无 error ✨"
  - confirmed + 全 ok → 计数 + 无 ⚠️ + /tasks / /retry hint
  - confirmed + 部分失败 → ⚠️ + 失败数

## Key design decisions

- **下划线 cancel_all_error 而非 dash**：TG setMyCommands API + 既有
  drift-defense 测试 `name must be lowercase ASCII / digit / underscore`
  要求 name 仅 [a-z0-9_]。dash 会让 registry 注册失败。
- **confirm token 必填**：与既有 /reset 单击立即生效不同 — reset 是
  软重置（清 LLM 上下文，可重建），本命令一次性破坏 N 条 task 状态
  且 cancel 无 retry 路径（要再创建）。token 是 friction 强制 owner
  确认意图。
- **case-insensitive confirm**：owner 大写敲 "CONFIRM" 应该接受 —
  全凭键盘大小写状态不该卡 UX。
- **非 confirm token 整体视作 unconfirmed**：owner 误敲 "yes" / "ok"
  时不该被算作确认。reply 重复展示完整命令 hint 教育正确用法。
- **task_cancel_inner 失败不阻断**：批量操作中并发 race 可能（owner
  同时在桌面改了状态 / 任务消失）— log err 计数继续后续，比"一条
  fail 全停"友好。
- **仅 cancel 本 chat origin 任务**：与 /cancel 同 scope 限制 —
  其它 chat / 桌面派单的 error 不受影响。reply 明确说明此 scope。
- **DecisionLogStore 走 state.app.state**：每条 cancel 推 decision_log
  记 audit；批量也每条记，让 owner 后续回看时能定位"哪批 cancel 是
  哪天哪个命令"。

## Verification

- `cargo test --lib`（backend）— 1258 passed / 0 failed（7 新 cancel_
  all_error 测试通过；两 drift-defense 也命中新加的 "cancel_all_error"）
- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.26s)
