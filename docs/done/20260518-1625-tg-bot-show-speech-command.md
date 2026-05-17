# TG bot `/show_speech [N]` 命令（iter #483）

## Background

iter #468 加 `/last_speech` 显 pet 最近 1 条主动开口。但 owner 想
audit「pet 最近一段时间说过啥 / 节奏 / 内容多样性」时需要看多条 —
要么逐条 /last_speech（只能拿 1 条无法翻页）要么切桌面 PanelDebug 看
recent speeches list。

本 iter 加 `/show_speech [N]` — 列最近 N 条主动开口（N 缺省 5 / clamp
1..=20）。与 /last_speech 单条对偶 — newest first，每行紧凑显示。

## Changes

### `src-tauri/src/telegram/commands.rs`

#### 1. `TgCommand::ShowSpeech { n: u32 }` 变体

紧贴 `LastSpeech`（同 speech-history 族）。

#### 2. 解析（与 /recent / /digest / /alarms 同 N-clamp 模板）

```rust
"show_speech" => {
    let n = title
        .split_whitespace()
        .next()
        .and_then(|s| s.parse::<u32>().ok())
        .map(|n| n.clamp(1, 20))
        .unwrap_or(5);
    Some(TgCommand::ShowSpeech { n })
}
```

#### 3. `format_show_speech_reply` pure 函数

```rust
pub fn format_show_speech_reply(entries: &[(String, String)]) -> String;
```

输入 `(ts_str, text)` tuples（由 handler `recent_speeches_with_meta(n)`
转换得到，oldest-first）；函数内 reverse 让 newest-first。

3 段：
- 空 → 「🗣 speech_history 空 — pet 还没主动开口过」兜底
- 标题行 `🗣 pet 最近 N 条主动开口（newest first）：`
- 每条 `· MM-DD HH:MM · <text 80 字 cap>`，超长 + …，flatten 换行

text 80 字 cap（per-row 紧凑 vs /last_speech 200 字单条完整）— 在 TG
单 reply 容量内 N=20 条仍可读。

### `src-tauri/src/telegram/bot.rs`

handler 紧贴 `LastSpeech`：

```rust
TgCommand::ShowSpeech { n } => {
    let entries = recent_speeches_with_meta(n as usize).await;
    let tuples: Vec<(String, String)> = entries
        .into_iter()
        .map(|e| (e.ts, e.text))
        .collect();
    format_show_speech_reply(&tuples)
}
```

复用既有 `recent_speeches_with_meta(n)` — 与 /last_speech / PanelDebug
recent-speeches chip 同 backend path。entries 的 meta 字段本命令不
用，但保 single-source 不引第二 read API。

### Registry + ALL_HELP_TOPICS + help-for-topic + table line + 2x drift defense

- 双 lang registry 各加一条（紧贴 last_speech）
- ALL_HELP_TOPICS 紧贴 "last_speech"
- format_help_for_topic 加详细文案 + /last_speech 交叉引用 + 反向
  cross-ref 在 /last_speech 末添加
- format_help_text 全表加 `/show_speech [N]` 一行
- 两处 drift-defense 测试列表加 "show_speech"

### 8 单元测试

- parse（默认 5 / 显参 / clamp 0/9999 / 垃圾 fallback）× 4
- format（empty / reverse-to-newest-first / 80-char truncate / newline
  flatten）× 4

## Key design decisions

- **N clamp 1..=20 缺省 5**：与 /recent / /recent_chats / /digest /
  /feedback_history / /alarms / /active_recent / /oldest_n 全部 N-cap
  命令统一上限。20 让 TG 单消息 ~4KB 装得下 + 屏幕可读
- **reverse to newest-first**：speech_history.log 是 append-only oldest-
  first；本命令显「最近 N 条」owner 心智应是 newest-first 看
- **80-char text cap per row**：比 /last_speech 单条 200 字短 — multi-row
  紧凑视图；超长 utterance 走 /last_speech 看完整
- **flatten newline**：speech 可能含换行（如 "你好\n我是 pet"），单行
  reply 视觉不被破坏；想看完整走 /last_speech
- **复用 recent_speeches_with_meta**：单 source-of-truth；meta 字段本命
  令不用但 entry struct 提供 ts+text 字段就够，新建独立 reader API 增
  复杂度无收益
- **不引 grouping by hour / day**：纯 list view 让 owner 自己用 ts 判断
  「这是早上的 vs 晚上的」；grouping 引入逻辑复杂度收益不显
- **不写 unit test on async handler**：handler 仅 stitching（await +
  map + invoke formatter）；formatter + parser 单测覆盖各 corner case。
  GOAL.md "meaningful tests only" 规则下不引装饰性 handler test

## Verification

- `cargo build --lib` — clean
- `cargo test --lib telegram::commands::tests::show_speech` — 4/4 通过
- `cargo test --lib telegram::commands::tests::format_show_speech` —
  4/4 通过
- `cargo test --lib`（全表）— 1571/1571 通过（+8 from 1563）
