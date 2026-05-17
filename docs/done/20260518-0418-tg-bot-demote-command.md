# TG bot `/demote <title>` 命令（priority -1）（iter #356）

## Background

iter #355 ship 了 `/promote` (+1)；本 iter 完成 `/demote` (-1) 对偶
命令。owner 想"这条不那么急了"一键降一阶，不必走 /pri 算具体 P 值。

## Changes

完全镜像 /promote 结构 — 仅算法 `saturating_add(1).min(9)` 换为
`saturating_sub(1)`，边界态从 P9 改为 P0：

### `src-tauri/src/telegram/commands.rs`

- enum `Demote { title: String }` 变体
- `name()` → "demote"；`title()` → title 字段（与 Promote 同桶）
- 解析器："demote" 分支与 /promote 同模板（所有 arg 当 title）
- 新 pure formatter `format_demote_reply(title, old_priority, save_ok)`：
  - 空 title → usage hint 含 /pri / /promote 互补释义
  - old == 0 → "已是 P0（最低）— 不再降" no-op 友好文案
  - old > 0 + Ok → "🎯 已降「title」P<old> → P<new>"
  - Err → "🎯 降 priority 失败：<msg>"
  - old=None fallback → "🎯 已降「title」" 简短
- registry zh + en 都加 ("demote", desc)
- format_help_text 全表加 `/demote <title>` 行（/promote 之后）
- format_help_for_topic 加 "demote" key + /pri / /promote 交叉引用
- ALL_HELP_TOPICS 加 "demote"
- 两 drift-defense 名单同步加 "demote"

### `src-tauri/src/telegram/bot.rs`

- 加 `TgCommand::Demote { title }` handler arm（在 Promote arm 之前）：
  - 空 title → format_missing_argument("demote")
  - else 三层 resolve title
  - 查 chat views 拿 old priority
  - old == 0 → short-circuit no-op formatter（不调 backend）
  - old > 0 → new = saturating_sub(1) → task_set_priority → formatter
  - old None → fallback formatter

### Tests（7 个新 unit test，镜像 promote）

- parser：title 正常 / 空
- formatter：
  - 空 title → usage hint + 互补 /pri / /promote
  - P0 → "已是 P0 不再降"
  - 正常 P5→P4 → "已降 P5 → P4"
  - 失败 → "降 priority 失败" + error
  - old=None fallback → "已降 t" 简短

## Key design decisions

- **`saturating_sub(1)` clamp**：u8 下溢防御 + Pri 下限双重保护。
  P0 已被 short-circuit 提前 return，所以 saturating_sub 实际不会触发
  underflow — 但保留防御写法防未来重构破坏。
- **P0 short-circuit**：与 /promote P9 短路同模式 — 避免无效 backend
  写 + friendly "已是 P0（最低）— 不再降" reply。owner 心智 "I'm at
  the bottom, no further drop possible".
- **完全镜像 /promote**：parser / handler / formatter / tests 都按
  "差异最小化"原则只换符号与边界字。让两命令的 maintainability 紧密
  绑定 — 改一个 pattern 另一个同步修。
- **不抽 helper 共用 promote / demote**：诱惑加 `format_priority_shift_
  reply(direction, ...)` 共享底层文案 — 但单独两 fns 让文案各自精确
  调（"升" vs "降"，"最高" vs "最低"），抽 helper 会用宏 / 大量字段，
  收益不抵复杂度。

## Verification

- `cargo test --lib`（backend）— 1272 passed / 0 failed（7 新 demote
  测试通过；两 drift-defense 也命中新加的 "demote"）
- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.27s)
