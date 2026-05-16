# TG bot `/find <keyword>` 命令（iter #261）

## Background

TG bot 已有 `/today` / `/recent` / `/tasks` / `/stats` 等查询命令，但都按
"时间段 / 状态" 维度切。owner 在外面想"找含某关键词的 task"（如"那条
Downloads"，"上周派的周报" 等）目前只能 `/tasks` 滚完整列表自己找。

本迭代加 `/find <keyword>`：在本 chat 派单中搜 title / raw_description 子串
（case-insensitive），返回至多 10 条命中（状态 emoji + 标题）。pending /
error 状态浮顶（活跃任务更可能是 owner 想找的），溢出 10 条加 hint。

## Changes

仅后端：

- `src-tauri/src/telegram/commands.rs`：
  - `TgCommand::Find { keyword: String }` 新变体；`name()` / `title()`（用
    keyword 共享 title 字段获取入口）同步
  - `parse_tg_command` 加 `"find"` 分支：所有 arg 当 keyword（含空格也保留，
    让 "/find 整理 Downloads" 命中包含该子串的 task）
  - registry zh+en 注册（在 /recent 之后）
  - `format_find_reply(views, keyword)`：
    - 空 keyword → usage hint
    - filter `title.contains(kw_lower) || raw_description.contains(kw_lower)`
    - 按 status rank（Pending=0 / Error=1 / Done=2 / Cancelled=3）二次排序
    - cap 10 + 命中数 header + 溢出 hint
    - 状态 emoji：🟢 pending / ⚠️ error / ✅ done / 🚫 cancelled
  - help text 多加一行
  - 7 个新单元测试覆盖：parse keyword / 空 keyword / 无命中 / title case
    insensitive / raw_description 命中 / pending 浮顶 / cap 10 + 溢出 hint

- `src-tauri/src/telegram/bot.rs`：`TgCommand::Find { keyword }` 分支调
  `format_find_reply`；views 走既有 `read_tg_chat_task_views(chat_id)`。

## Key design decisions

- **case-insensitive lowercase compare**：UTF-8 lowercase 在 ASCII 段总能
  fold；中文不受影响（owner 写"Downloads" vs "downloads" 都能命中）。
- **匹配 title + raw_description 两段**：title 是常规命中点，raw_description
  含 markers / tag / origin / [result:] 等，让 owner 搜"#健身" / "[origin:tg"
  都能用同一命令。
- **pending / error 浮顶**：活跃 task 更可能是 owner 当下想要的；done /
  cancelled 沉底但仍展示，让 owner 想找"上次取消的那个" 也命中。
- **cap 10 而非 5 或 20**：5 太少（keyword 模糊时命中多），20 太长（单 TG
  消息 4096 字符限制 + 视觉滚屏）。10 是 owner 在 TG 上能扫读的合理上限。
- **空 keyword 走 formatter 内 usage hint 而非 missing-argument**：与
  `/pin` 等 require-title 命令不同，这里把例子文案直接显，owner 不必再去
  /help 查语法。

## Verification

- `cargo check` ✅
- `cargo test`（含 7 新测试 + 全表 1045 通过）✅
- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.21s)
