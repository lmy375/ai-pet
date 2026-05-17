# TG bot `/last` 命令（闪查最近创建的 task）（iter #325）

## Background

owner 在 TG 端 `/task <title>` 创任务后想"确认一下我刚加的内容对不
对"，目前只能：
- `/tasks` 拉全清单 → 从一堆任务里找最新
- `/show <title>` 但需要记得 title

都不够快。本迭代加 `/last` — 无参，自动找本聊天最近 `created_at` 的一
条 task 显出来，含 status emoji + 相对时间 + raw_description 前 200 字。

## Changes

### `src-tauri/src/telegram/commands.rs`

- enum `Last` 无参变体（与 `Mood` / `Today` / `Now` 同模板）
- `name()` → "last"；`title()` 归入无参桶
- 解析器："last" 分支无参；多余尾部忽略
- 新常量 `LAST_RAW_DESC_PREVIEW_CHARS = 200`
- 新 pure formatter `format_last_reply(views, now)`：
  - views 空 → 友好兜底（"还没派过单 · 用 /task 创第一条"）
  - `max_by(a.created_at.cmp(&b.created_at))` 找最新（ISO 字典序 = 时间
    序）
  - 头部 `🆕 最近创建 <emoji> 「<title>」\n📅 <相对时间>`
  - raw_description trim 后非空 → 加 200 char preview 段；空 → 省略
    （避免末尾留空双换行）
- 新 pure helper `format_created_relative(created_at, now) -> String`：
  - 走 `chrono::DateTime::parse_from_rfc3339 → naive_local` + signed_
    duration_since(now)
  - 桶式：< 60s → "刚创建"；< 60min → "N 分钟前"；< 24h → "N 小时前"；
    else → "N 天前"
  - parse 失败 → "created_at parse 失败：<raw>" hint（防御文案）
- registry zh + en 都加 ("last", desc)
- format_help_text 全表加 `/last` 行（/now 之后）
- format_help_for_topic 加 "last" key + /show / /recent / /tasks 交叉
  引用
- ALL_HELP_TOPICS 加 "last" 让 /help all 长版说明书包含
- 两 drift-defense 名单同步加 "last"

### `src-tauri/src/telegram/bot.rs`

- 加 `TgCommand::Last` handler arm（在 Now arm 之前）：
  - `read_tg_chat_task_views(chat_id.0)`（已 chat-scoped）
  - `chrono::Local::now().naive_local()` 注入 now 给 formatter
  - 调 `format_last_reply` 一次性

### Tests（11 个新 unit test）

- parser：无参 / 多余尾部容忍 / 大小写不敏感
- format_last_reply：
  - 空 views → 兜底文案 + 教育文案
  - max_by created_at 选最新（验证 3 条任务里只渲最新那条）
  - 四 status 各自 emoji（⏳ / ✅ / ⚠️ / 🚫）
  - 长 raw_description 截断 + ellipsis
  - 空 raw 省略段（无空双换行污染）
- format_created_relative buckets:
  - "刚创建"（< 60s）
  - "N 分钟前"
  - "N 小时前"
  - "N 天前"
  - parse 失败 hint

## Key design decisions

- **`max_by created_at` 而非 `min` / first / 数组首项**：created_at ISO
  字典序与时间序一致 — 走 `cmp` 标准比较拿最新。不依赖 backend
  task_list 返回顺序（哪种排序都不影响 /last 结果）。
- **`format_created_relative` 后端独立实现而非调前端 helper**：本是 bot
  端 pure formatter，不依赖任何前端代码；写法与前端 formatRelativeAge
  完全一致桶（分 / 小时 / 天）让 owner 在桌面 / TG 看到的"相对时间"
  文案一致。
- **raw_description preview 200 char**：与 /show 命令 (`SHOW_RAW_DESC_CAP
  = 1500`) 互补 — /last 是"快速 verify 刚创了啥"，cap 较紧凑让 TG 消息
  不超 2 行卡片；想看完整 raw 走 `/show <title>` 拿 1500。
- **空 raw 省略段**：极端情况（手动 task_create 跳过 description 等），
  preview 段不渲染保持 reply clean — 头部"最近创建 + title + 相对时间"
  已足够 verify。
- **`format_created_relative` 已 pure but 也用在前端? 否**：未在前端复
  用 — 前端有自己的 `formatRelativeAge`。但语义对齐，未来如想 sync
  两份实现可以引相同算法 const（不在本 iter scope）。

## Verification

- `cargo test --lib`（backend）— 1184 passed / 0 failed（11 新 /last
  测试通过；两 drift-defense 也命中新加的 "last"）
- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.24s)
