# TG bot `/here` 命令（iter #382）

## Background

iter #381 加 `/aware` — pet 视角 dump（pet 感知到什么）。本 iter 加
对偶的 `/here` — owner 视角 dump（owner 输入了什么）。完成
audit "我说啥 → pet 看啥 → pet 怎么反应" 全链路的两端信号镜。

## Changes

### `src-tauri/src/telegram/commands.rs`

#### 1. enum 变体（~line 213）

```rust
Here,
```

无参 — 与 /aware / /now 同 N-less 簇。

#### 2. `name()` → "here"；`title()` arm 加入无参簇

#### 3. parser（~line 870）

`"here" => Some(TgCommand::Here)` — 多余尾部一律忽略。

#### 4. `format_here_reply` pure formatter

```rust
pub fn format_here_reply(
    transient: Option<(&str, i64)>,
    mute_remaining_minutes: Option<i64>,
    band: &str,
) -> String
```

输出 4 行：
```
🧑 当前 owner 信号：
📝 transient_note: 「<text>」（剩 N 分钟）/ 未设
🔕 mute: 剩 N 分钟 / 未静音
💬 最近 feedback band: <label> · <factor 说明>
```

band 4 态分流（与 feedback_history::classify_feedback_band 同 enum
返值，未识别 fallback 到 insufficient_samples）：
- `high_negative` → "cooldown ×2.0（pet 更克制）"
- `low_negative` → "cooldown ×0.7（pet 更主动）"
- `mid` → "cooldown ×1.0（中性）"
- `insufficient_samples` → "样本不足 — cooldown 走基础值"

transient text 长 60 字截断；mins / mute_minutes clamp 最小 1 防过
期边界 "剩 0 分"。

#### 5. registry zh + en + format_help_text + format_help_for_topic
+ ALL_HELP_TOPICS + 两 drift-defense 列均加 "here"

### `src-tauri/src/telegram/bot.rs`

新 handler arm（紧贴 Aware 之前）：

```rust
TgCommand::Here => {
    let (tn_text, tn_until) = proactive::get_transient_note();
    let transient = parse_to_opt_tuple(tn_text, tn_until);
    let mute_remaining_minutes = proactive::mute_remaining_seconds()
        .map(|secs| ((secs + 59) / 60).max(1));
    let entries = feedback_history::recent_feedback(20).await;
    let (band, _factor) = feedback_history::classify_feedback_band(&entries);
    format_here_reply(transient, mute_remaining_minutes, band)
}
```

复用既有 read 路径：
- `proactive::get_transient_note()` → (text, until_iso)
- `proactive::mute_remaining_seconds()` → Option<i64>
- `feedback_history::recent_feedback(20)` → Vec<FeedbackEntry>
- `feedback_history::classify_feedback_band(&entries)` → (&str, f64)

mute seconds → minutes 用 `((secs + 59) / 60)` ceil 到上整数分钟；
`.max(1)` clamp 防 secs ∈ (0, 60) 边界态显 "剩 0 分钟"。

### Tests（commands.rs，9 个新 unit test）

Parser（2 个）：
- 无参 / 多余尾部忽略

Formatter（7 个）：
- 全 signal 渲染（transient + mute + high_negative band + factor）
- 无 signal → 全 baseline 文案
- low_negative band → "更主动" + ×0.7
- mid band → "中性" + ×1.0
- mute=0 → clamp "剩 1 分钟"
- 长 transient → "…" 截断
- 未识别 band → fallback insufficient_samples

## Key design decisions

- **4 行 snapshot 而非长描述**：owner 一眼看完三类信号 — transient
  写入 / mute 强信号 / band 隐性聚合。与 /aware 5 行（pet 视角含
  时间 / 陪伴 / mood / tasks / transient）对称密度。
- **band factor 在文案里翻译成"更克制 / 更主动 / 中性"**：raw band
  string 对 owner 不直观 — pet 行为变化的描述更可操作（"哦原来我
  high_negative 让 pet 现在更克制"）。R7 cooldown adapter 的
  factor 数字也保留让懂的人对照。
- **未识别 band fallback 到 insufficient_samples**：defensive — 万
  一 `classify_feedback_band` 返新 string（未来扩 enum）formatter
  不崩；显 baseline 文案。
- **mute seconds → minutes ceil 而非 floor**：剩 90 秒 owner 心智
  "剩 2 分钟"更合理（reservoir 多于一分钟 → 显 2 而非 1）；clamp
  到 1 防 < 60s 显 0。
- **不显 feedback aggregate detail（replied/ignored/dismissed 计数）**：
  那是 /feedback_history 的 job；本命令仅 band level summary。help
  text 交叉引用 /feedback_history 让 owner 跳详情。
- **保 pure formatter + caller 注入所有 IO 结果**：与 /aware /
  /now formatter 同模式 — unit test 不依赖运行时全局 mutex。

## Verification

- `cargo check`（backend）— clean
- `cargo test --lib`（backend）— **1345 passed / 0 failed**（+9 新
  here test，两 drift-defense 列也命中 "here"）
- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.25s)
