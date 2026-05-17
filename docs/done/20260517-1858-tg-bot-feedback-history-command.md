# TG bot `/feedback_history [N]` 命令（iter #370）

## Background

owner 写过的 /feedback comment + 系统记录的隐性反馈（回复 / 点掉
bubble / 👍 / 沉默忽略 / 🤔 puzzled）都在 feedback_history.log，但
此前只有桌面 PanelDebug feedback timeline 卡片能看。手机端要 audit
"我给 pet 留过什么 / pet 接收了哪些训练信号" 需要回桌面。

本 iter 加 TG bot `/feedback_history [N]` 命令打通手机端 audit 路
径，与 /feedback（写）对偶 — 与 iter #363/#364（写） 思路一致：写
入入口先于回看入口。

## Changes

### `src-tauri/src/telegram/commands.rs`

#### 1. enum 变体（~line 165）

```rust
FeedbackHistory { n: u32 },
```

#### 2. `name()` / `title()` arms

- name → "feedback_history"
- title() → "" (无参；空段语义)

#### 3. parser（~line 999）

与 /digest / /recent 同 clamp 模板：N 缺省 5，clamp 1..=20，非数字
尾部走默认。

#### 4. `format_feedback_history_reply`（pure，~line 2431）

入参 `entries: &[FeedbackEntry]`（newest-first，由 bot handler 把
recent_feedback().await 的 oldest-first 反转）+ `n: u32`：
- 空 entries → 友好兜底文案 + 引导 /feedback 写第一条
- 非空 → header `📜 最近 N 条 feedback：` + 逐行 `· HH:MM <emoji>
  <kind> | <excerpt>`
- 超出 N 时显 overflow hint "还有 X 条更早记录"

kind emoji 映射：
- ✅ replied · 👍 liked · 💬 comment（owner 主动正面 / 正面 / 评论）
- 🙉 ignored · 👋 dismissed · 🤔 puzzled（被动负 / 主动负 / 困惑）

#### 5. registry zh + en + format_help_text + format_help_for_topic
+ ALL_HELP_TOPICS + 两 drift-defense 列均加 "feedback_history"

### `src-tauri/src/telegram/bot.rs`

新 handler arm（紧贴 Transient 之前）：

```rust
TgCommand::FeedbackHistory { n } => {
    let mut entries = crate::feedback_history::recent_feedback(n as usize).await;
    entries.reverse();
    crate::telegram::commands::format_feedback_history_reply(&entries, n)
}
```

复用既有 `feedback_history::recent_feedback(n)` async 函数读取
`feedback_history.log`。

### Tests（commands.rs，9 个新 unit test）

Parser（5 个）：
- 默认 N=5 / 显式 N=10 / clamp 上限 (999 → 20) / clamp 下限 (0 → 1) /
  非数字 fallback

Formatter（4 个）：
- 空 entries → bootstrap usage hint
- 正常渲染（含两种 kind emoji 命中）
- N cap + overflow hint
- 短 timestamp fallback（< 16 chars 不 panic，全字符串显）

## Key design decisions

- **入参 newest-first 而非内部 sort**：让 bot handler 显式控制顺序，
  formatter 保 pure。如未来引入 oldest-first 显示偏好（PanelDebug
  timeline 已用 reverse），单独 reverse 调用前置即可，不动 formatter。
- **kind emoji 集中映射**：6 类反馈用 6 个 emoji 不歧义 — 与桌面
  PanelDebug feedback timeline 行 emoji 风格一致（让 cross-surface
  阅读心智一致）。
- **不暴露 in-memory aggregate（high_negative 等）**：那些是
  format_feedback_aggregate_hint 的 LLM 心智，不是 owner 直观信息。
  owner 想看趋势走 PanelDebug。
- **excerpt 不二次截断**：feedback_history.log 写入时已 cap 64 字
  （FEEDBACK_EXCERPT_CHARS），N=20 × 90 char ≈ 1800 < TG 4096 limit
  安全。
- **clamp 1..=20 与 /digest / /recent 同**：保 TG 命令 N-style 语义
  一致；owner 心智 "数字命令 N 缺省 5 上限 20"。
- **`title()` arm 加入空段 (无参) 簇**：与 Digest / Recent / Mute
  等 N-only 命令同列 — name() 唯一，title() 返 ""。

## Verification

- `cargo check`（backend）— clean
- `cargo test --lib`（backend）— **1293 passed / 0 failed**（+9 新
  feedback_history test，两 drift-defense 列也命中 "feedback_history"）
- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.23s)
