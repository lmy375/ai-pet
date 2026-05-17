# TG bot `/pri <title> <N>` 命令（slim priority 修改）（iter #343）

## Background

`/edit <title> :: <new desc>` 全量覆写 description — 想改 priority 时
owner 需要重写整个 desc 包括所有 markers / body，太重。`task_set_priority`
后端早已支持单字段修改（PanelTasks 行右键 priority 子菜单走的就是它），
但 TG 端没暴露。

本迭代加 `/pri <title> <N>` 一行命令单改 priority，复用既有
`task_set_priority` 后端 — 保 due / body / 其它 markers 全不动。

## Changes

### `src-tauri/src/telegram/commands.rs`

- enum 加 `Pri { title: String, priority: Option<u8> }`（priority Option
  让 parser 失败时仍能进 handler 走 usage hint）
- `name()` → "pri"；`title()` → title 字段
- 解析器：rsplit 末 whitespace token 当 priority u8 (≤9)，剩余作 title：
  - 解析成功 + 0..=9 → priority = Some(n)
  - 解析失败 / 越界 → priority = None，全段作 title
  - 单 token / 空字符串 → priority = None
- 新 pure formatter `format_pri_reply(title, priority, save_ok)`：
  - title 空 / priority None → usage hint with examples
  - Ok(()) → "🎯 已设「<title>」P<N>"
  - Err(msg) → "🎯 改 priority 失败：<msg>"
- registry zh + en 都加 ("pri", desc)
- format_help_text 全表加 `/pri <title> <N>` 行（/streak 之后）
- format_help_for_topic 加 "pri" key + 例子 + 与 /edit / /done / /cancel
  交叉引用
- ALL_HELP_TOPICS 加 "pri"
- 两 drift-defense 名单同步加 "pri"

### `src-tauri/src/telegram/bot.rs`

- 加 `TgCommand::Pri { title, priority }` handler arm（在 Edit arm 之前）：
  - 空 title / 无 priority → formatter usage hint
  - 否则 try_resolve_by_index → resolve_tg_task_title 三层 resolve
  - 调 `task::task_set_priority(resolved_title, priority)`
  - 成功 / 失败都走 format_pri_reply 文案

### Tests（11 个新 unit test）

- parser：
  - 标准 "title N"
  - title 含空格（"整理 Downloads 桌面 7" → "整理 Downloads 桌面" + 7）
  - boundary N=0 / N=9
  - 越界 N=10 → priority None + 全段作 title
  - 无末数字 → priority None + 全段作 title
  - 空 / 单 token → priority None
- formatter：
  - 空 title → usage hint
  - 仅 title 无 priority → usage hint
  - 成功 reply 含 🎯 / "已设" / title / "P<N>"
  - 失败 reply 含 "改 priority 失败" + error msg

## Key design decisions

- **priority: Option<u8> 而非 u8**：parser 失败时仍能构造 TgCommand（带
  None）让 handler 走 usage hint 路径，避免 Unknown command 回退。与
  /due 的 `preset: Option<DuePreset>` 同模板。
- **rsplit 末 whitespace token**：title 含空格是常态（"整理 Downloads
  桌面"），priority 是单字符 / 双字符末段。比首空白切分稳定；与 /snooze
  trailing preset 同 parser 模式。
- **越界 N (> 9) 整段当 title**：parser 不强 reject — 让 owner 可以
  把"长 11 char" 之类当 title 末尾输错时 handler 走 usage hint 比 silent
  error 更友好。
- **复用 task_set_priority 而非新 backend**：单字段修改 — task_set_
  priority 已经处理 legacy task (parse header / fallback header /
  TASK_PRIORITY_MAX 检查 / decision_log 跳过 — 与 PanelTasks 行内 picker
  同后端，行为完全一致)。
- **三层 resolve 复用既有 pattern**：与 /done / /cancel / /edit 同
  ergonomic — owner 不必为 /pri 学新引用方式。
- **不引入 emoji P-pill picker**：保持 reply 紧凑单行 "🎯 已设「t」P5"
  — 与 format_command_success 系列同风格，不打扰 owner。

## Verification

- `cargo test --lib`（backend）— 1238 passed / 0 failed（11 新 pri 测
  试通过；两 drift-defense 也命中新加的 "pri"）
- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.23s)
