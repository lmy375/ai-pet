# TG bot `/edit <title> :: <new desc>` 命令（iter #300）

## Background

owner 在 TG 端已有 `/task` 创建、`/done` `/cancel` `/retry` 状态切换、
`/snooze` `/pin` `/silent` markers 切换、`/note` 记 general 等命令，但缺
"改 butler_task body" 入口 —— 想加个 marker（如 `[deadline: …]`）、改
拼写、补充上下文，只能回桌面 PanelMemory 改。

本迭代加 `/edit` 让 owner 在 TG 单行命令搞定 "覆写 description 整段"。

## Changes

### `src-tauri/src/telegram/commands.rs`

- enum `TgCommand` 加 `Edit { title: String, new_desc: String }` 变体
- `name()` → `"edit"`；`title()` → `title.as_str()`
- 解析器："edit" 分支用 `title.split_once("::")` 切分；无 separator 时
  整体当 title、new_desc 空（让 handler 走 usage hint）
- `format_edit_reply(title, new_desc, save_result)` 纯文案 formatter：
  - 任一端空 → usage hint（带 `::` 例子 + 全量覆写 caveat）
  - Ok(()) → "✏️ 已覆写「<title>」" + 80 字预览
  - Err(msg) → "✏️ 覆写失败：<msg>"
- `tg_command_registry_localized` zh + en 都加 ("edit", desc)
- `format_help_text` 全表加 `/edit ... 描述` 行（/digest 之后、/reset 之前）
- `format_help_for_topic` 加 "edit" key 含详细用法 + 示例 + 注意事项
- drift-defense 测试 `format_help_for_each_listed_command_returns_detail`
  名单加 "edit"
- 9 个新 unit test 覆盖：
  - 标准 split / `::` 出现多次只切首个
  - 无 separator → new_desc 空
  - 仅 `::` / 一端为空的退化场景
  - usage hint / partial missing arg / 成功 preview / long desc 截断 /
    save failure error 文案
- `tg_command_registry_covers_all_user_facing_commands` 名单加 "edit"

### `src-tauri/src/telegram/bot.rs`

- 加 `TgCommand::Edit { title, new_desc }` handler arm（在 Note arm 之前）：
  - 空 title / 空 new_desc → `format_edit_reply` 走 usage hint 路径
  - 否则用 `try_resolve_by_index` → `resolve_tg_task_title` 三层 resolve
    （与 /done /cancel /retry 同 pattern）
  - 命中 → `memory_edit("update", "butler_tasks", title, Some(new_desc), None)`
  - 成功 / 失败分支都走 `format_edit_reply` 文案

## Key design decisions

- **`::` 分隔符而非首空白切**：butler_task title 经常含空格（"整理
  Downloads"）/ 中文标点 / 全角符号 —— 首空白切会切错。`::` 比 `|` 更
  显眼、比 `\n` 更显式（owner 一眼看出"两半"），与既有 butler_history
  log 格式 `<action> <title> :: <desc>` 文化对齐。
- **全量覆写 vs 增量改**：增量改（如"加一个 marker"）会牵出"在哪行 / 在
  哪 marker 之间插"的歧义；全量覆写是单一意图，owner 知道自己写什么就
  得到什么。新 desc 中既有 markers 由 owner 自行写进 —— 与桌面 detail.md
  textarea save 语义一致。help 文案明确提醒 markers 需自带。
- **不复用 /pin /silent 等专用 marker 命令**：那些是"幂等切换 marker
  状态"；/edit 是"全量改 body"，正交语义。owner 想加单个 marker → 走
  专用命令；想多处改 → 走 /edit 一次性写完。
- **resolve 与 /done /cancel 同三层**：数字 index → fuzzy → 错误候选。
  一致 UX —— owner 不必为不同命令记不同的"如何引用 task"。
- **memory_edit 直调而非 task_update_inner**：butler_task 是 memory
  category，update 走 memory_edit("update")；task_mark_done_inner 等是
  status 切换专用，不适合"全文覆写描述"语义。memory_edit 已 hook 进
  SQLite mirror + butler_history log，调用一次全套副作用都跟进。

## Verification

- `cargo check`（backend）— clean，仅既有 dead_code warnings
- `cargo test --lib`（backend）— 1111 passed / 0 failed（9 新 edit 测试都
  通过；drift-defense 测试也命中新加的 "edit"）
- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.22s)
