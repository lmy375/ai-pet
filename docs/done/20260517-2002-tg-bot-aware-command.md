# TG bot `/aware` 命令（iter #381）

## Background

owner 想 debug "pet 为啥没主动开口 / 选了那条 task" 时，需手动看
PanelToneStrip（transient_note）+ PanelMemory（active tasks）+
PanelPersona（mood）+ PanelDebug（companionship）等多处。手机端尤
其麻烦。

本 iter 加 TG `/aware` 一句话 dump pet 当前感知 snapshot — 含
transient_note 这条 /now 不显的关键调度信号。与 /now（仅时间 +
mood emoji，最简）/ /whoami（多行画像 + 自我介绍长文）互补，是
"中等粒度感知 snapshot"。

## Changes

### `src-tauri/src/telegram/commands.rs`

#### 1. enum 变体（~line 205）

```rust
Aware,
```

无参 — 与 /now / /last / /streak 等同 N-less 簇。

#### 2. `name()` → "aware"；`title()` arm 加入无参簇

#### 3. parser（~line 868）

`"aware" => Some(TgCommand::Aware)` — 多余尾部一律忽略（与 /now 同
模板）。

#### 4. `format_aware_reply` pure formatter

```rust
pub fn format_aware_reply(
    transient: Option<(&str, i64)>,          // (text, remaining_minutes)
    active_count: usize,                     // butler_tasks 非 [done] 数
    mood_text: Option<&str>,                 // 心情文本，None / 空 → 走兜底
    now: chrono::DateTime<chrono::FixedOffset>,
    companionship_days: Option<u64>,
) -> String
```

输出 5 行 snapshot：
```
🐾 当前感知：
📝 transient_note: 「<text>」（剩 N 分钟） / 无
📋 active tasks: N 条
☁ mood: <emoji> <text> / 🐾 （暂无心情）
🕐 当前: YYYY-MM-DD HH:MM (+08:00) · 陪伴 N 天 / 今日初识
```

设计要点：
- transient text 超 60 字 → "<head>…" 截断
- remaining_minutes clamp 最小 1（防"剩 0 分钟"边界过期态）
- mood 空 / 仅空白 → emoji 🐾 + "（暂无心情）" 兜底（避免 mood 段
  整个消失让 owner 困惑 "哪里少了一行"）
- companionship_days = 0 → "今日初识"；> 0 → "陪伴 N 天"；None → 整
  尾部省略

#### 5. registry zh + en + format_help_text + format_help_for_topic
+ ALL_HELP_TOPICS + 两 drift-defense 列均加 "aware"

### `src-tauri/src/telegram/bot.rs`

新 handler arm（紧贴 Today 之前 / Now 之后）：

```rust
TgCommand::Aware => {
    let now_local = chrono::Local::now();
    let now_fixed = now_local.with_timezone(now_local.offset());
    let companionship_days = Some(companionship::companionship_days().await);
    let mood = mood::read_current_mood_parsed();
    let mood_text = mood.as_ref().map(|(t, _)| t.as_str());

    let (tn_text, tn_until) = proactive::get_transient_note();
    let transient = if tn_text.is_empty() {
        None
    } else {
        let mins = parse_from_str(&tn_until, ...).map(...).unwrap_or(0);
        Some((tn_text.as_str(), mins))
    };

    let active_count = memory_list("butler_tasks").map(|index|
        index.categories["butler_tasks"].items.iter()
            .filter(|it| !it.description.contains("[done]"))
            .count()
    ).unwrap_or(0);

    format_aware_reply(transient, active_count, mood_text, now_fixed, companionship_days)
}
```

复用既有 read 路径：
- `proactive::get_transient_note()` (text, until_iso)
- `companionship::companionship_days()` async u64
- `mood::read_current_mood_parsed()` Option<(text, motion)>
- `memory_list("butler_tasks")` + filter `!description.contains("[done]")`

### Tests（commands.rs，8 个新 unit test）

Parser（2 个）：
- 无参 / 多余尾部忽略

Formatter（6 个）：
- 全 signal 渲染（transient + tasks + mood emoji + time + companionship）
- 空 transient → "无"
- 0 companionship → "今日初识"
- mood 仅空白 → emoji 🐾 + "暂无心情" 兜底
- 长 transient → "…" 截断
- mins=0 → clamp "剩 1 分钟"
- companionship=None → 尾部仅时间 + tz，省"陪伴"段

## Key design decisions

- **5 行 snapshot 而非 1 行**：1 行（/now 模式）信息密度太高 — 含
  transient_note 长文本时尤其挤。5 行各管一类信号，owner 一眼分辨
  哪类有问题。
- **保 pure formatter + 注入 now/days/...**：与 /now formatter 同模
  式 — unit test 不依赖运行时 Local::now() / file IO。
- **transient `remaining_minutes` 让 caller 算 vs formatter 算**：
  caller 算 用 chrono::Duration（含 sub-minute 精度信息）；formatter
  收 i64 minute 简化签名 + 易测。
- **不引入 "下次 due" 字段**：TODO 写了但实际计算"全 butler_tasks
  next-fire-time" 需引 butler_schedule 整套依赖。scope 守住"active
  count"足够，owner 想看 next-due 走 /due 或 /today。
- **mood 仅空白也显 🐾 + 兜底文案**：与 /now 同 "emoji 兜底"风格但
  本命令多带显式 "（暂无心情）" 让 owner 知道字段被检测但内容缺。
- **不嵌套 sub-commands（/aware brief / /aware verbose 等）**：scope
  守住单一 snapshot；想要更细信号走 /whoami（画像）+ /tone（chip
  全量，桌面 PanelToneStrip 同源）— 桌面有了，TG 暂不复制。

## Verification

- `cargo check`（backend）— clean
- `cargo test --lib`（backend）— **1336 passed / 0 failed**（+8 新
  aware test，两 drift-defense 列也命中 "aware"）
- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.26s)
