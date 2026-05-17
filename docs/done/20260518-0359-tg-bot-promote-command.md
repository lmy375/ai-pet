# TG bot `/promote <title>` 命令（priority +1）（iter #355）

## Background

`/pri <title> <N>` 绝对设值需要 owner 算"目标 P 值"（如 P3 想升一阶
是 P4 还是 P5？）。owner 经常想"这个更急一些 / 这个不那么急"相对调整 —
应该有相对值入口。本迭代加 `/promote` (+1) 单字符相对动作 — 复用
task_set_priority 后端保 due / body / 其它 markers 不动。

(`/demote` -1 反方向命令是下一个 TODO 项。)

## Changes

### `src-tauri/src/telegram/commands.rs`

- enum `Promote { title: String }` 变体
- `name()` → "promote"；`title()` → title 字段
- 解析器："promote" 分支与 /cancel / /done 同模板（所有 arg 当 title）
- 新 pure formatter `format_promote_reply(title, old_priority, save_ok)`：
  - 空 title → usage hint 含 /pri / /demote 互补释义
  - old == 9 → "已是 P9（最高）— 不再升" no-op 友好文案
  - old < 9 + Ok → "🎯 已升「title」P<old> → P<new>"
  - Err → "🎯 升 priority 失败：<msg>"
  - old=None fallback（view miss）→ "🎯 已升「title」" 简短
- registry zh + en 都加 ("promote", desc)
- format_help_text 全表加 `/promote <title>` 行（/cancel_all_error 之后）
- format_help_for_topic 加 "promote" key + /pri / /demote 交叉引用
- ALL_HELP_TOPICS 加 "promote"
- 两 drift-defense 名单同步加 "promote"

### `src-tauri/src/telegram/bot.rs`

- 加 `TgCommand::Promote { title }` handler arm（在 CancelAllError arm
  之前）：
  - 空 title → format_missing_argument("promote")
  - else 三层 resolve title (try_resolve_by_index → resolve_tg_task_title)
  - 命中后查 chat views 找 current priority
  - old == 9 → short-circuit no-op formatter（不调 backend 省一次写）
  - old < 9 → new = saturating_add(1).min(9) → task_set_priority →
    formatter (Ok / Err)
  - old None (view miss 兜底) → 仍调一次 set 但不知道 old，formatter
    走 fallback 简短文案

### Tests（7 个新 unit test）

- parser：title 正常 / 空
- formatter：
  - 空 title → usage hint + 互补 /pri / /demote
  - P9 → "已是 P9 不再升"
  - 正常 P3→P4 → "已升 P3 → P4"
  - 失败 → "升 priority 失败" + error
  - old=None fallback → "已升 t" 简短

## Key design decisions

- **`saturating_add(1).min(9)` clamp**：u8 加法溢出 + Pri 上限双重保护。
  老 backend `task_set_priority` 内部也校验 `priority > TASK_PRIORITY_MAX`
  reject — 但前端 clamp 让 happy path 不撞那条 reject 错误。
- **P9 short-circuit no-op**：避免无效 backend 调用 + 友好 reply 让
  owner 知道"已经到顶了"而非看到 "升 priority 失败：priority must be
  0..=9 (got 10)" 错误信息。
- **复用 `task_set_priority`**：与 PanelTasks 行内 picker / /pri TG 命
  令同后端。保 due / body / 其它 markers / detail.md 全套不动。
- **view miss fallback**：极少 race（resolve 成功但 views 查不到 —
  task 状态在 resolve / lookup 间被改了），fallback 走简短 "已升 t"
  reply 仍可读。
- **不引新 backend 命令**：promote / demote 都是"读 → +1/-1 clamp →
  写"组合，frontend 计算 + 单次 set_priority 调用足够。引"task_inc_
  priority" 等单独后端命令是 over-engineering。

## Verification

- `cargo test --lib`（backend）— 1265 passed / 0 failed（7 新 promote
  测试通过；两 drift-defense 也命中新加的 "promote"）
- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.24s)
