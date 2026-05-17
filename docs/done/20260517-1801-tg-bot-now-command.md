# TG bot `/now` 命令（一句话快速状态 check）（iter #319）

## Background

`/whoami` 输出多行画像（陪伴 / 心情 / 自我画像首段 / top tools），适合
owner audit "宠物全景"。但日常想"现在几点 / 宠物啥状态"闪查时多行响应
反而冗。`/mood` 又只显心情，不带时间 / tz / 陪伴。

本迭代加 `/now` —— 一两行紧凑回复：mood emoji + 当前时间 + tz + 陪伴
天数 + 心情文本。与 /whoami 多行画像互补，定位"快速 status check"。

## Changes

### `src-tauri/src/telegram/commands.rs`

- enum `Now` 无参变体（与 `Mood` / `Today` / `Markers` 同模板）
- `name()` → "now"；`title()` 归入无参桶
- 解析器："now" 分支无参；多余尾部一律忽略
- 新 pure formatter `format_now_reply(now, companionship_days, mood_text)`：
  - 第一行：`{mood_emoji_for(mood) or 🐾} {YYYY-MM-DD HH:MM} ({+TZ:HH})`
  - 第二行（可选）：`陪伴 N 天 · 心情：<text>` — 任一缺时省略对应段
  - days == 0 切 "今天与你初识" 与既有 /whoami pattern 同
  - mood 文本 trim 后空走 None 路径（兜底 🐾）
  - now 参数是 `DateTime<FixedOffset>` 便于单测注入确定 tz
- registry zh + en 都加 ("now", desc)
- format_help_text 全表加 `/now` 行（/today 之后）
- format_help_for_topic 加 "now" key + 与 /whoami / /mood 交叉引用
- ALL_HELP_TOPICS 加 "now" 让 /help all 长版说明书包含
- 两 drift-defense 名单同步加 "now"

### `src-tauri/src/telegram/bot.rs`

- 加 `TgCommand::Now` handler arm（在 Today arm 之前）：
  - `chrono::Local::now()` 拿本地时间 → `with_timezone(now.offset())`
    得 `DateTime<FixedOffset>` 给 pure formatter 注入
  - 复用 `companionship::companionship_days()` async read
  - 复用 `mood::read_current_mood_parsed()` 同 /whoami / /mood

### Tests（7 个新 unit test）

- parser：无参 / 多余尾部容忍 / 大小写不敏感
- formatter：
  - full signal（time + tz + days + mood）渲染完整
  - mood "今天很开心" → 😊 prefix（验证复用 mood_emoji_for）
  - mood = None → 🐾 兜底 + 不显心情段
  - mood = "   " 空白 trim → 🐾 兜底
  - companionship_days = 0 → "今天与你初识" 文案
  - companionship_days + mood 都缺 → 仅时间单行
  - 负 tz offset (-05:00) 正确渲染

## Key design decisions

- **time-line 格式 `YYYY-MM-DD HH:MM (+TZ)` 而非更长 RFC3339**：完整
  日期 + 分钟精度 + tz 偏移对 owner 已足够（不必秒 / 微秒）。`+08:00`
  比 `Asia/Shanghai` 更紧凑且通用（不依赖 owner 知道 tz 名 → offset 映射）。
- **复用 mood_emoji_for（iter #311 helper）**：与 /whoami 头部 emoji 同源
  让"宠物心情可视化"在两条命令视觉一致。avoid 写第二份 emoji 映射表
  drift 风险。
- **两段 join with ` · ` separator**：陪伴 + 心情两段在同一行用紧凑分隔，
  保 reply ≤ 2 行短小。
- **now 参数走 FixedOffset 注入而非内部 Local::now**：pure formatter 单
  测能注入确定时间（如 2026-05-17 14:42 +08:00）做断言；bot.rs 端调用
  时 wrap `Local::now()` 走 with_timezone 转 FixedOffset 即可。
- **mood text trim 后空 → 🐾 + 不显心情段**：mood state 文件可能存在
  但 text 为空（异常 / 初次状态）— 防御处理避免显 "心情：" 后空白。
- **不带 task counts / persona / tools**：那些是 /whoami 域。本命令只
  覆盖时间 + 陪伴 + 心情三个 axis — 让 owner 知道"宠物在场 + 心情如何 +
  几点"。

## Verification

- `cargo test --lib`（backend）— 1173 passed / 0 failed（7 新 now 测试
  通过；两 drift-defense 也命中新加的 "now"）
- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.22s)
