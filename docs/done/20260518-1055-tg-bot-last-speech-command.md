# TG bot `/last_speech` 命令（iter #468）

## Background

iter #462 加 ChatMini 顶部「⏱ pet 沉默 N 分」chip — 自上次 pet 主动
开口算分钟数，让 owner 觉察「pet 是不是又卡住了」。但 chip 只显沉默
**时长**，不显**最近说了啥** — owner 想看「上次具体什么时刻 / 内容」
要切 chat scroll 找 ts。

本 iter 加 TG `/last_speech` — 显 pet 最近一条主动开口的 ts + 文本 +
相对时间。与 ChatMini ⏱ 沉默 chip 对偶（chip 显时长，命令显内容）。

## Changes

### `src-tauri/src/telegram/commands.rs`

#### 1. `TgCommand::LastSpeech` variant

紧贴 `Now`（同 "single-shot ambient signal" 族）。无参；`name() = "last_speech"`；
title() 空（与 /now / /aware 同 no-title 组）。

#### 2. parser

```rust
"last_speech" => Some(TgCommand::LastSpeech),
```

多余 trailing 一律忽略（与 /now / /aware / /here 同容忍模板）。

#### 3. `format_last_speech_reply` pure 函数

```rust
pub fn format_last_speech_reply(
    entry: Option<(&str, &str)>,
    now: chrono::DateTime<chrono::Local>,
) -> String;
```

入参 `Option<(ts_str, text)>` 由 handler 已 await
`recent_speeches_with_meta(1)` 准备好；`now` 锚点用 inject 让单测稳定。

4 态：
- `None` → 「🗣 pet 还没主动开口过」+ usage hint（推 /aware / /here）
- parse ts 失败 → 「🗣 pet 最近主动开口：「<text>」（ts 解析失败：<raw_ts>）」
- 成功 → 「🗣 pet 最近主动开口 · MM-DD HH:MM（N 分前 / N 小时前 / N 天前）：\n「<text 前 200 字>」」

text 截 200 字 cap + 末尾 "…" hint（与 /last task preview 同 cap）。

### `src-tauri/src/telegram/bot.rs`

handler 紧贴 `Now`：

```rust
TgCommand::LastSpeech => {
    let entries = crate::speech_history::recent_speeches_with_meta(1).await;
    let entry_opt = entries.first().map(|e| (e.ts.as_str(), e.text.as_str()));
    let now = chrono::Local::now();
    crate::telegram::commands::format_last_speech_reply(entry_opt, now)
}
```

- 复用既有 `recent_speeches_with_meta(1)` — 已 production 验证（PanelDebug
  最近开口 chip 同 backend path）
- `.first()` 取最新（function 已按 oldest-first 排序，但 n=1 时唯一一
  条就是最新）
- now 注入 pure formatter — handler 是 IO 边界，formatter 单测无时钟
  耦合

### Registry + ALL_HELP_TOPICS + help-for-topic + table line + 2x drift defense

- 双 lang registry 各加一条（紧贴 now）
- ALL_HELP_TOPICS 紧贴 "now"
- format_help_for_topic 加详细文案；同步在 /now 详细文案末追加交叉引用
- format_help_text 全表加 `/last_speech` 一行
- 两处 drift-defense 测试列表加 "last_speech"

### 7 单元测试

- parse（no-args / trailing-ignored）× 1
- format（None 兜底 / minutes 相对 / hours 相对 / days 相对 / 200 字
  truncate / invalid ts fallback）× 6

## Key design decisions

- **3 段相对时间格式（分 / 小时 / 天）**：与 ChatMini ⏱ chip 同 tiered
  display — `N 分前` < 1h；`N 小时前` 1..24h；`N 天前` ≥ 24h。让 owner
  一眼看「最近还是很久」节奏感
- **200 字 text cap + "…"**：与 /last task preview 200 字 cap 同协议；
  longer speeches 在 TG bubble 渲染丑+读不完，cap 兜底
- **invalid ts fallback 仍显 text**：极少 case（speech_history.log 被
  外部工具篡改）但兜底防 100% 错误反馈 — 至少 owner 看到「最近开口
  内容」+「ts 不可信」hint
- **`now` 作为 inject 参数**：handler 是 IO 边界 + 注入 chrono::Local::now()；
  formatter 完全 pure 不读时钟 — 单测每条用 fixed_local() 注入确定
  ts 让 assertion 稳定
- **复用 `recent_speeches_with_meta(1)` 而非新 lookup**：既有函数已包
  含 ts 提取 + meta join；本命令 meta 用不上但 entry struct 已含 ts +
  text 字段就够 — 一处函数多处用减 drift
- **handler async + tokio::fs::read_to_string**：speech_history 函数本
  就 async；handler 直接 await 不阻塞 bot 事件循环
- **不写 unit test on async handler**：handler 是简单 stitching（await
  + struct unpack + invoke formatter）；formatter 单测覆盖所有
  branches。GOAL.md "meaningful tests only" 规则下不引装饰性 handler
  test
- **与 ChatMini ⏱ chip 对偶不重复信号**：两 surface 互补 — chip 显
  「pet 已沉默多久」紧凑 ambient 信号；命令显「最近开口具体内容 + 时
  刻」详细 audit 入口。owner 在 TG 端看到 chip 警示后用 /last_speech
  追细节

## Verification

- `cargo build --lib` — clean
- `cargo test --lib telegram::commands::tests::last_speech` — 7/7 通过
- `cargo test --lib`（全表）— 1543/1543 通过（+7 from 1536）
- handler 实际场景（pet 主动说一句 → 等 5 分钟 → TG `/last_speech`）
  应返回该消息 + ts + "5 分前"
