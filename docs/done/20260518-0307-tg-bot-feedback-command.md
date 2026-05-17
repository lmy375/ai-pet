# TG bot `/feedback <text>` 命令 + FeedbackKind::Comment 变体（iter #352）

## Background

owner 想给 pet 说「你最近太啰嗦了」/「这次选 task 选得很到位」/「周末
别打扰我」这种直接调整行为反馈时，当前只能：
- 桌面 ChatMini bubble 上点 👍 / 🤔 / 主动点掉 — 但都是 sentiment chip
  不能传文字
- 在 chat 里聊（耗 LLM token + pet 不一定记住为 feedback）

本迭代加 TG `/feedback <text>` 命令 + 新 `FeedbackKind::Comment` 变体 ——
owner 主动 text 反馈直接写 feedback_history.log，LLM 在下次 proactive
cycle 读到 owner 原话调整。

## Changes

### `src-tauri/src/feedback_history.rs`

- enum `FeedbackKind` 加 `Comment` 变体 — owner-initiated 中性反馈（情
  感倾向不预判，让 LLM 看原文判断）
- `as_str()` 加 `"comment"` 映射
- `parse_line()` 加 `"comment" => FeedbackKind::Comment` 反向 parse
- 聚合 hint (`format_aggregate_hint` line 358-365)：
  - 新 `comment` 计数器
  - 显「N 留言反馈」when > 0（位于 liked 之后、puzzled 之前）
- `format_feedback_hint`（latest entry 文案）：
  - 加 `Comment => "owner 通过 /feedback 留言: ... — 直接读 owner 原话
    调整后续行为（可能是正向 / 负向 / 中性建议，按字面 + 上下文判断）"`

### `src-tauri/src/telegram/commands.rs`

- enum `TgCommand::Feedback { text: String }` 变体
- `name()` → "feedback"；`title()` → text 字段
- 解析器："feedback" 分支与 /note / /reflect 同模板（所有 arg 当 text）
- 新 pure formatter `format_feedback_reply(text)`：
  - 空 text → usage hint 含 /note / /reflect 对比 + 三个示例
  - 非空 → "💬 已记到 feedback_history" + 60 char preview + 提示 pet 下
    次会读到
- registry zh + en 都加 ("feedback", desc)
- format_help_text 全表加 `/feedback <text>` 行（/pri 之后）
- format_help_for_topic 加 "feedback" key + /note / /reflect 交叉引用
- ALL_HELP_TOPICS 加 "feedback"
- 两 drift-defense 名单同步加 "feedback"

### `src-tauri/src/telegram/bot.rs`

- 加 `TgCommand::Feedback { text }` handler arm（在 Pri arm 之前）：
  - 空 text → formatter usage hint
  - 否则 best-effort `feedback_history::record_event(Comment, trimmed)`
  - 调 `format_feedback_reply(&text)` 反馈

### Tests（5 个新 unit test）

- parser：text 正常 / 空
- formatter：
  - 空 text → usage hint 含 /note / /reflect 对比
  - 非空 → preview + "pet 下次主动开口前会读到"
  - long text 截断 + ellipsis

## Key design decisions

- **新 `Comment` 变体而非 reuse Liked**：用 Liked 会让"owner 说太啰
  嗦了"被 LLM 当正向信号 — 完全反语义。Comment 中性 + 走 format_feedback_
  hint 专属文案让 LLM 看 owner 原话自己判断。
- **影响 4 个 match 点**：as_str / parse_line / format_aggregate_hint /
  format_feedback_hint。每点都加新 arm 完整覆盖。verifier：cargo test
  全套 1251 passed 无回归。
- **format_aggregate_hint 中 comment 位置**：在 liked 之后、puzzled 之
  前 — 顺序按"主动正向 → 主动反馈 → 困惑（中性弱信号）→ ignored → 主
  动负向"语义递降。
- **best-effort record_event**：`feedback_history::record_event` 已是
  fire-and-forget 设计（log 失败不阻塞主流）— bot.rs 不 unwrap 不
  branch，reply 始终走 success path。
- **preview 60 char**：与 /note (60) / /reflect (60) 同 cap 让三个 text
  类 reply 紧凑一致。
- **/help 详情含对比 /note / /reflect**：避免 owner 选错入口 — 三命令
  按存储目的分流（杂项 → general / 反思 → ai_insights / 行为调整反馈
  → feedback_history）。

## Verification

- `cargo test --lib`（backend）— 1251 passed / 0 failed（5 新 feedback
  测试通过；FeedbackKind 既有测试也仍通过 — 4 个 match 点全覆盖；两
  drift-defense 命中新加的 "feedback"）
- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.23s)
