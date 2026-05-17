# TG bot `/show <title>` 命令（iter #309）

## Background

owner 在 TG 端有 `/tasks` 看清单 / `/find` 搜任务 / `/done` `/cancel`
状态切换 / `/edit` 全量覆写描述。但缺"我想细看这条 task 当前 raw
description 和 detail.md 内容"的 audit 入口 —— 想看 markers 组合 / 进度
笔记必须回桌面 PanelTasks。

本迭代加 `/show <title>` 让 owner 在 TG 一行命令拉单条任务详情。

## Changes

### `src-tauri/src/telegram/commands.rs`

- enum 加 `Show { title: String }` 变体
- `name()` → "show"；`title()` 归入 title 桶
- 解析器："show" 分支 = title 全文（与 /cancel /done 同模板）
- 新常量：`SHOW_RAW_DESC_CAP = 1500` / `SHOW_DETAIL_PREVIEW_CHARS = 300`
- 新 pure formatter `format_show_reply(title, raw_description, detail_md, status)`：
  - title 行 + status emoji（⏳ pending / ✅ done / ⚠️ error / 🚫 cancelled）
  - raw_description trim 全量 + 1500 char cap + 截断时显总字符数
  - detail.md 段：空 → 省略；非空 → 前 300 字符 + 总字符数 hint
  - 空 raw_description 兜底文案
- registry zh + en 都加 ("show", desc)
- format_help_text 全表加 `/show <title>` 行（/find 之后）
- format_help_for_topic 加 "show" key + /find 详情 cross-reference 加
  「/show（看单条详情）」
- 两个 drift-defense 名单同步加 "show"

### `src-tauri/src/telegram/bot.rs`

- 加 `TgCommand::Show { title }` handler arm（在 Edit arm 之前）：
  - 空 title → format_missing_argument("show")
  - 否则 try_resolve_by_index → resolve_tg_task_title 三层 resolve
  - 命中后从 read_tg_chat_task_views 查 status，从 task_get_detail 拉
    raw_description + detail_md
  - 调 format_show_reply 一次性输出

### Tests

- 9 个新 unit test：
  - parse：title 正常 / 空 title
  - reply：四种 status 各自 emoji / 短 raw_description 不截断 / 长 raw
    截断 + 总字符数 hint / detail.md 非空显 preview + 字符数 / 空 detail
    省略段 / 长 detail 截断 + 省略号 / 空 raw 兜底文案
- 两 drift-defense 名单 (format_help_for_each_listed_command +
  tg_command_registry_covers) 都加 "show"

## Key design decisions

- **raw_description cap=1500 / detail=300**：TG message hard limit 4096
  字符；raw + detail + 头部 ~50 字符总和应留 buffer。raw 更重要（含
  markers 完整组合）所以给 1500；detail 是 preview 性质给 300（owner 想
  看全文可回桌面 PanelTasks）。
- **status emoji 从 read_tg_chat_task_views 查而非 task_get_detail
  返回**：task_get_detail 当前不返 status；从 views 查也能拿到（task 必
  在 view list 里，三层 resolve 已保证）。view miss 时 fallback Pending
  防 panic。
- **detail.md 空段省略**：owner 任务多无 detail（butler_tasks 新建后 0
  字符）；空段渲染 "📝 detail.md（0 字符）:\n" 是噪音。空时直接不渲染
  段头。
- **三层 resolve 复用既有 pattern**：与 /done /cancel /edit /retry 同
  入口语义 — owner 不必为 /show 记不同的"如何引用 task"。
- **不返 history 段**：task_get_detail 同时拉 butler_history 但 owner
  看历史走 /tasks（已有 stat）或 /digest（含 result）。/show 专注"this
  task 当前状态" — history 是另一维度，加进来让消息显著变长。
- **不分页**：第二条 /show 才有意义；当下 cap 已经覆盖 99% 任务。owner
  极端长任务（> 1500 char raw）回桌面 detail 阅读，反而是合理的体验信号。

## Verification

- `cargo test --lib`（backend）— 1147 passed / 0 failed（9 新 show 测试
  通过；drift-defense 也命中新加的 "show"）
- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.22s)
