# TG bot `/digest [N]` 命令（iter #291）

## Background

`/recent N` 已经能列最近 N 条 done task 标题，但 owner 经常想"扫读最近做
了啥 + 具体产物" — 标题之外还想看 `[result: ...]` 摘要。比如：
- "整理 Downloads · 挪了 30 个文件"
- "跑步 · 5km" 
- "写文档 · 周三 standup 提案"

`/recent` 只显标题；`/digest` 同范围但每条加一行式 result 摘要。

## Changes

仅后端：

- `src-tauri/src/telegram/commands.rs`：
  - `TgCommand::Digest { n: u32 }` 新变体；`name() / title()` 同步
  - `parse_tg_command` 加 `"digest"` 分支：与 `/recent` 同 N 处理（缺省 5,
    clamp 1..=20, 非数字 fallback 5）
  - registry zh+en 注册（在 /note 后）
  - `format_digest_reply(views, n)`：
    - filter `TaskStatus::Done`
    - sort `updated_at` desc（最新在前）
    - take n
    - 单行格式：`· MM-DD HH:MM · title — result`；result 缺时省 `—` 段；
      result 截 80 字（unicode-aware）+ `…`
    - 空 done → friendly hint + `/digest` 用法
    - overflow → "…还有 N 条更早完成（/digest N 看更多）"
  - help text 多加一行
  - `format_help_for_topic` 加 `digest` 详细文案
  - drift-defense test 命令列表加 `"digest"`
  - 10 个新单测覆盖：parse default / explicit / clamp 边界 / 非数字 fallback /
    empty done / 倒序 + result 显 / 跳非 done / done 无 result 不显 `—` /
    长 result 截 80 字 / overflow hint

- `src-tauri/src/telegram/bot.rs`：`Digest { n }` 分支调
  `format_digest_reply`；views 走既有 `read_tg_chat_task_views`。

## Key design decisions

- **与 /recent 互补而非取代**：纯标题更紧凑（多数情况看时间 + 主题足以
  recall），含 result 更详细（适合"上周做了啥"复盘）。owner 在不同场景
  选不同命令。
- **80 字 result 截断**：TG 单条消息 4096 字符上限；多条 task 累加 result
  到 80 字保持紧凑，hover detail.md / 桌面 panel 看完整 result。
- **`—` 分隔符**：result 段用 em dash 与 title 分隔，比 `:` / `·` 视觉
  更明显是"摘要 / 注解"形态。无 result 时 dash 段也省，避免悬挂。
- **复用 chat-scoped read path**：与 /recent / /tasks / /today / /find /
  /blocked / /snoozed 同源 — 行为一致。

## Verification

- `cargo check` ✅
- `cargo test`（含 10 新 /digest 测试 + 全表 1102 通过）✅
- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.20s)
