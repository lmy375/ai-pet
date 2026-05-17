# TG bot `/transient <text> [minutes]` 命令（iter #363）

## Background

owner 想从手机给宠物写"我在开会"、"集中写文档不要打扰"、"今晚 9
点后再 ping 我" 这类**临时上下文**。proactive::set_transient_note
后端已有（R55，桌面 PanelToneStrip 显示这条 [临时指示]），但**只
能从 Rust 内部调**——没 Tauri 命令暴露给前端，TG bot 也无入口。

与 /note（→ general memory **存盘**）/ /reflect（→ ai_insights 存
盘）/ /feedback（写 feedback_history.log 改 pet 行为）/ /mute（直
接静音）四个写入入口对偶 — 本命令是「给 pet 临时上下文，**不存盘
只挂 in-memory**，到时自动清除，不阻塞 pet 开口」的独特通道。

## Changes

### `src-tauri/src/telegram/commands.rs`

#### 1. enum 变体（~line 161）

```rust
Transient { text: String, minutes: i64 },
```

#### 2. `name()` / `title()` arms

- `TgCommand::Transient { .. } => "transient"`
- `title()` 加入 text-bearing 共用 arm

#### 3. parser（~line 909）

末 whitespace token 若 parse 为 `i64 ∈ 1..=10080` → minutes；越界 /
解析失败 → 整段当 text + default 60。仅 1 个 token 时**不**当 minutes
解析（与 /pri 同模板 — 避免 `/transient 30` 误丢标题）。空 text → `text=""`,
`minutes=60` 让 handler 走 usage hint。

#### 4. `format_transient_reply`（~line 2557，pure）

输入 `(text, minutes, until_local: Option<DateTime<Local>>)`：
- 空 text → 多行 usage hint，含示例 + 与 /note / /reflect /
  /feedback / /mute 的对比说明
- 正常 set → "📝 已设 transient_note（N 分钟有效）\\n\\n<preview>\\n\\n到 HH:MM 自动清除"
- until=None defensive fallback → 省 "到 HH:MM" 段（理论不该触发，但
  proactive::set_transient_note 内部 lock 失败时 until_iso 为空）
- nice duration label：复用与 format_mute_reply 同分桶（< 60min /
  < 24h / ≥ 24h），让两命令风格统一
- preview 60 char cap（与 feedback_history truncate 风格一致）

#### 5. registry zh + en（~line 430 / ~line 477）

```
("transient", "Set a transient note for N minutes — in-memory only context for the pet (default 60m, cap 7d)")
("transient", "设 N 分钟有效的临时上下文给 pet（不存盘 in-memory；缺省 60m，上限 7 天）")
```

#### 6. `format_help_text` 全表（~line 1393）+ `format_help_for_topic` 详细文案（~line 1328）+ `ALL_HELP_TOPICS` 加 "transient"

#### 7. 两 drift-defense 测试名单（~line 3711, ~line 4177）加 "transient"

### `src-tauri/src/telegram/bot.rs`

新 handler arm（~line 1145）：
- 空 text → formatter usage hint，不调 backend
- 非空 → `proactive::set_transient_note(trimmed, minutes)` 返 ISO
  until 字符串 → parse 回 `DateTime<Local>` 给 formatter 渲染 HH:MM；
  parse 失败 → None fallback（防御）

### Tests（commands.rs，12 个新 unit test）

Parser（6 个）：
- text + minutes 正常
- text 不带 minutes → default 60
- 单 token text（非数字 / 数字）都当 text（不解析）
- minutes 越界（> 10080 / 0 / 负） → 整段当 text + default 60
- 空命令 → 空 text + 60
- 上限 10080 合法

Formatter（6 个）：
- 空 text → usage hint + 含 /note / /mute 对比
- 含 until → "已设" + text + "N 分钟" + "HH:MM"
- hour label（90 / 120 分钟分桶）
- day label（4320 分钟 → "3 天"）
- 长 text → preview 截断 "…"
- until=None defensive fallback → 无"到 — 自动清除"占位

## Key design decisions

- **不抽 helper 共用 `format_mute_reply` 的 duration 文案**：诱惑
  把 "N 分钟 / N 小时 N 分钟 / N 天 N 小时" 抽通用 fn — 但两命令
  的 reply 心智不同（mute 强调静音 / transient 强调临时指示），
  分别内联让 25 行各自直白可读。如未来第三个 minutes-based 命令
  再抽。
- **`minutes ∈ 1..=10080` clamp**：与 /mute 同上限 7 天。0 不接
  受 — set_transient_note(0) 会清除 note 但 TG 命令的意图是 set，
  不该让 0 当 clear（owner 想清除应等过期或加专门 /transient_clear
  命令；目前不优先）。
- **单 token 不解析为 minutes**：与 /pri 同模板 — `/transient 30`
  owner 大概率是想"set transient 内容为'30'"而非"set transient
  minutes=30 但漏 text"。单 token 数字也按 text，让 formatter 走
  usage hint（emptiness branch 不触发但其它 branch 显"已设 …
  「30」… 60 分钟"，owner 看到"我哪里漏了"自然会改）。
- **defensive until=None fallback**：proactive::set_transient_note
  内部 `if let Ok(mut g) = TRANSIENT_NOTE.lock()` lock 失败时
  until_iso 返空字符串。parse 失败 → None。formatter 仍给可读
  reply，不崩。理论上 lock 永远拿得到，但写防御代码保 bot 不被
  edge case 撞挂。
- **不暴露桌面前端 Tauri command**：TODO 列表里有姐妹任务
  "PanelToneStrip 加「✍️ 写 transient_note」按钮" — 那个是单独 iter
  会做。本 iter 严格限于 TG 路径，避免双 surface 同时改增加 review
  成本。

## Verification

- `cargo check`（backend）— clean，仅遗留 dead-code warnings
- `cargo test --lib`（backend）— **1284 passed / 0 failed**（+12
  新 transient test；drift-defense 两测试也命中新加的 "transient"）
- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.23s)
