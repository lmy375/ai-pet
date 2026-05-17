# TG bot `/random` 命令（随机抽 active 任务）（iter #326）

## Background

owner 在 TG 看到一堆 active 任务（pending / error）时偶尔陷入"选择困难
我先做哪个"的状态。`/tasks` 看全清单 / `/today` 看今日 due / `/blocked`
看被锁住的 都是 audit 视角，没有"帮我决定" 视角。

本迭代加 `/random` — 从本 chat 派单的 active 任务（pending / error）里
随机抽 1 条，附带 "—— 选择困难？就先做这条吧。" 鼓励文案，让 pet 帮
owner 跨过 decision paralysis。

## Changes

### `src-tauri/src/telegram/commands.rs`

- enum `Random` 无参变体（与 `Now` / `Last` / `Today` 同模板）
- `name()` → "random"；`title()` 归入无参桶
- 解析器："random" 分支无参；多余尾部忽略
- 新常量 `RANDOM_RAW_DESC_PREVIEW_CHARS = 200`
- 新 pure formatter `format_random_reply(views, index_seed)`：
  - filter pending / error active 任务（done / cancelled 不在 candidates）
  - 空 candidates → 兜底文案 "暂无 active 任务可抽 · 用 /task 创"
  - `candidates[index_seed % candidates.len()]` 选一条
  - 头部 `🎲 抽中 <emoji> 「<title>」（共 N 条 active）`
  - raw_description trim 后非空 → 加 200 char preview 段
  - 尾部固定鼓励文案 "—— 选择困难？就先做这条吧。"
- registry zh + en 都加 ("random", desc)
- format_help_text 全表加 `/random` 行（/last 之后）
- format_help_for_topic 加 "random" key + 与 /tasks / /blocked / /today
  交叉引用
- ALL_HELP_TOPICS 加 "random" 让 /help all 长版说明书包含
- 两 drift-defense 名单同步加 "random"

### `src-tauri/src/telegram/bot.rs`

- 加 `TgCommand::Random` handler arm（在 Last arm 之前）：
  - `read_tg_chat_task_views(chat_id.0)`（已 chat-scoped）
  - `SystemTime::now().duration_since(UNIX_EPOCH).subsec_nanos() as usize`
    当 seed — 非确定性体验 + 不需引入 rand crate
  - 调 `format_random_reply` 一次性

### Tests（9 个新 unit test）

- parser：无参 / 多余尾部容忍 / 大小写不敏感
- formatter：
  - 全 done / cancelled → 兜底文案 + 教育文案
  - 只 pick pending（done / cancelled 不进 candidates）
  - error 状态也算 active（含 ⚠️ emoji）
  - seed 索引确定性 — seed 0/1/2/3 验证（包含 wrap-around `3 % 3 = 0`）
  - 显 active count（共 N 条）
  - 长 raw_description 截断 + ellipsis
  - 空 raw 不产生空段（验证无多余空行）
  - 尾部鼓励文案存在

## Key design decisions

- **seed 索引而非 `rand::random()`**：避免引入 rand crate 依赖；调用方
  传 seed 让单测确定行为（同 seed 同结果）。bot.rs 用 system time
  nanos 当 seed → 实际使用拿到"不可预测体验"，单测用 0/1/2/3 等小数验
  证索引算法正确。
- **active = pending + error 不含 cancelled / done**：cancelled / done
  是终态 — 让 owner "随机做"它们没意义。error 含 in 让 owner 能被 "随
  机指派" 一条 errored task 来 retry，与 pending 一样的"待办" 语义。
- **尾部鼓励文案"选择困难？就先做这条吧"**：与 owner 走 /random 的语境
  共鸣（owner 不是想随机看任务，是想被推一把决定）。短句不卖弄。
- **不显 priority / due**：本命令是"随机推荐" — 信号是 task 本身。
  priority / due 详情走 /show <title> 二次查；本 reply 紧凑专注 raw
  description preview。
- **`seed: usize`（不 i64 / u64）**：直接用 `% candidates.len()` 不
  cast；usize == native pointer width 与 Vec len 同型号最自然。

## Verification

- `cargo test --lib`（backend）— 1193 passed / 0 failed（9 新 random
  测试通过；两 drift-defense 也命中新加的 "random"）
- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.22s)
