# TG bot `/yesterday` 命令（昨日产出 audit）（iter #337）

## Background

owner 在 TG 端有 `/today`（今日 due / done 切片）+ `/recent N`（不限日
期最近 N 条 done）+ `/digest N`（最近 N done 含 result）。但缺单条
"昨天做完了啥" 的简单 audit 入口 —— owner 早晨复盘 / 周一回顾上周日产
出时需要走 /recent 30 然后心算"哪些是昨天的"。

本迭代加 `/yesterday` 一句话列昨日 done 任务 + result 摘要，与 /today
对偶。

## Changes

### `src-tauri/src/telegram/commands.rs`

- enum `Yesterday` 无参变体（与 Today / Now / Mood 同模板）
- `name()` → "yesterday"；`title()` 归入无参桶
- 解析器："yesterday" 分支无参；多余尾部一律忽略
- 新 pure formatter `format_yesterday_reply(views, today)`：
  - `today - chrono::Duration::days(1)` 算 yesterday boundary
  - filter Done + `updated_at.starts_with(yesterday)` （ISO YYYY-MM-DD
    前缀匹配）
  - 按 updated_at desc 排（最新完成在前）
  - 头部 `📅 昨日（YYYY-MM-DD）完成 N 条：` + 每条 `· ✅ <title>` +
    optional ` — <result 前 40 字>`
  - 空 → 友好兜底 `昨日（...）无完成记录 · /recent / /today 替代提示`
  - result trim 空白 → 不渲染 segment（避免末尾 " — " 空尾巴）
- registry zh + en 都加 ("yesterday", desc)
- format_help_text 全表加 `/yesterday` 行（/quick 之后）
- format_help_for_topic 加 "yesterday" key + /today / /recent / /digest
  交叉引用
- ALL_HELP_TOPICS 加 "yesterday"
- 两 drift-defense 名单同步加 "yesterday"

### `src-tauri/src/telegram/bot.rs`

- 加 `TgCommand::Yesterday` handler arm（在 Quick arm 之前）：
  - `read_tg_chat_task_views(chat_id.0)` (已 chat-scoped)
  - `chrono::Local::now().date_naive()` 注入 today
  - 调 `format_yesterday_reply` 一次性

### Tests（7 个新 unit test）

- parser：无参 / 多余尾部容忍 / 大小写不敏感
- formatter：
  - 空 → "无完成记录" + /recent hint
  - 过滤准确（only Done + only on yesterday — 今日 / pending / cancelled
    都不进 candidates）
  - 排序按 updated_at desc 验证（3 条任务时间不同）
  - result summary 渲染
  - 长 result truncate + ellipsis
  - 空白 result trim 后无 " — " 段

## Key design decisions

- **`today - 1 day`** 在 formatter 内部算而非 caller 传 yesterday：caller
  传 today 与 `/today` / `/due` 命令一致；formatter 内 -1 day 让单测稳
  定（同 today 注入同结果）。
- **filter Done only**：与 /today done 段同语义 — pending / error /
  cancelled 不进 "完成记录" 维度。owner 想 audit 昨日 cancelled 走
  其它入口（未来可能加 /cancelled-yesterday 等，但 scope 不扩张）。
- **`updated_at.starts_with("YYYY-MM-DD")`** 前缀匹配 ISO 字符串：与
  format_today_reply 同算法 — 不必 parse 时间，直接字典前缀检查。
- **result preview 40 char**：比 /digest 的 default 短（/digest 内 result
  无截断 — 不同命令定位）；/yesterday 是"昨日清单"短答场景，每条文案
  紧凑。
- **空白 result trim 后省略 " — "**：防御文案污染 — 末尾出现 `· ✅ t — `
  对 owner 是噪音。trim 后空就直接不渲段。
- **不显 created_at 等元数据**：reply 紧凑 — owner 在 TG 想看 detail
  走 /show <title>。

## Verification

- `cargo test --lib`（backend）— 1215 passed / 0 failed（7 新 yesterday
  测试通过；两 drift-defense 也命中新加的 "yesterday"）
- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.19s)
