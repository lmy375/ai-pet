# TG bot `/streak` 命令（完成节奏 audit）（iter #339）

## Background

owner 在 TG 端 audit 完成数据：/today 看今日 / /yesterday 看昨日 /
/recent N 看最近 N / /digest N 含 result。但缺"完成节奏" 维度信号 —
连续做了几天 / 近 7-30 天总完成量。这种 streak 数据对 habit-tracking
意义重大（让 owner 看到"我已经连 5 天有 done 了 — 别打破"）。

本迭代加 `/streak` —— 三个 pure helpers + 友好包装，告诉 owner 连续 done
天数 + 近 7 / 30 天 done 总数。

## Changes

### `src-tauri/src/telegram/commands.rs`

- enum `Streak` 无参变体（与 Yesterday / Today / Now 同模板）
- `name()` → "streak"；`title()` 归入无参桶
- 解析器："streak" 分支无参；多余尾部一律忽略
- 三个新 pure helper（让 streak 逻辑可单测 + 未来 PanelDebug 复用）：
  - `done_dates_from_views(views) -> HashSet<NaiveDate>`：filter Done +
    parse updated_at[..10] 为 NaiveDate；非 done / parse 失败跳过
  - `compute_done_streak(set, today) -> u32`：
    - 空集 → 0
    - streak 末端：今日有 → today；否则若昨日有 → yesterday；否则 0
    - 从末端往前数连续天数（gap 即 break）
  - `count_done_in_window(views, today, days) -> u32`：算
    [today - (days-1), today] 闭区间内 done 总数
- `format_streak_reply(views, today)`：connect 三个 helpers + 渲染：
  - streak > 0 → "🔥 连续 N 天有完成"
  - streak = 0 → "🌱 streak 中断 — 今日 / 昨日均无完成"
  - 末行 "📊 近 7 天 done：N 条 · 近 30 天 done：M 条"
- registry zh + en 都加 ("streak", desc)
- format_help_text 全表加 `/streak` 行（/yesterday 之后）
- format_help_for_topic 加 "streak" key + /today / /yesterday / /stats
  交叉引用
- ALL_HELP_TOPICS 加 "streak"
- 两 drift-defense 名单同步加 "streak"

### `src-tauri/src/telegram/bot.rs`

- 加 `TgCommand::Streak` handler arm（在 Yesterday arm 之前）：
  - `read_tg_chat_task_views(chat_id.0)`（已 chat-scoped）
  - `chrono::Local::now().date_naive()` 注入 today
  - 调 `format_streak_reply` 一次性

### Tests（9 个新 unit test 覆盖三个 helpers + 包装）

- parser：无参 / 多余尾部容忍 / 大小写不敏感
- compute_done_streak：
  - 空集 → 0
  - 仅今日 → 1
  - 仅昨日 → 1（验证今日缺时从昨日起算）
  - 连续 3 天到今日 → 3
  - gap 打断 streak（今日有 + 前天有，昨日缺 → 1）
  - 都没今日 / 昨日（仅 3 天前）→ 0（防"虚 streak"）
- done_dates_from_views：filter Done + 多 status 验证
- count_done_in_window：7d / 30d 闭区间边界 / 非 done 不进
- format_streak_reply：streak > 0 显 🔥 / streak = 0 显 🌱 + 计数行

## Key design decisions

- **streak end at today OR yesterday**：避免"今日还没开始做"时 streak
  报 0 — 多数 owner 中午前 audit 时今日还没 done 但 streak 应该仍算
  active。fallback 到昨日是常见 habit-app 行为（Streaks app / Duolingo
  等同 pattern）。
- **3 个 pure helpers 独立可测 + 单测齐备**：streak 算法易出 off-by-one
  / boundary bug — 拆分让每个核心步骤可单独验证（done_dates →
  compute_done_streak 闭区间 → count_done_in_window 7d/30d 边界）。
- **count_done_in_window 算 instances 不算 unique days**：owner 同一天
  完成 3 条 task → 7 天 done = 3 不是 1。让数字与"我这周完成几条"直觉
  一致。如未来要加"高效天数"维度可独立 helper。
- **`days >= 1` 时闭区间 `[today - (days-1), today]`**：days=7 → 含今
  日共 7 天。常见 "近 7 天" 心智。
- **status emoji 🔥 vs 🌱**：streak > 0 用 fire（保持的 streak）；
  streak = 0 用 seedling（重新开始 — 鼓励而非批评）。与既有 status
  emoji（⏳ pending / ✅ done / ⚠️ error）色调一致。
- **caller 注入 today**：所有 helpers / formatter 都不读时钟 — 单测注
  入固定 date 拿确定结果。bot.rs 端 chrono::Local::now().date_naive()
  转换。

## Verification

- `cargo test --lib`（backend）— 1227 passed / 0 failed（9 新 streak
  helper / formatter 测试通过；两 drift-defense 也命中新加的 "streak"）
- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.22s)
