# TG bot `/snooze_until <title> <HH:MM>` 命令（iter #481）

## Background

TG 已有：
- `/snooze <title> [preset]` — 相对预设（30m / 2h / tonight / tomorrow /
  monday 等）
- `/sleep_until <HH:MM>` — 绝对时刻静音 proactive 整体

但缺一条对偶：**绝对时刻 snooze 单条 task**。owner 想「这条 task 暂
到下午 6 点」时只能心算"还剩 N 分钟"再 /snooze N — friction。

本 iter 加 `/snooze_until <title> <HH:MM>` — title + HH:MM rsplit
parser + 与 /sleep_until 同跨日规则（目标 ≤ now → 明日同时刻） +
复用 task_set_snooze 后端。

## Changes

### `src-tauri/src/telegram/commands.rs`

#### 1. `TgCommand::SnoozeUntil { title: String, time: Option<(u8, u8)> }` 变体

紧贴 `SleepUntil`（同 absolute-time 对偶族）。`time: Option` 让 parser
解析失败也能保留 title 让 handler 走 usage hint。

#### 2. 解析

```rust
"snooze_until" => {
    let s = title.trim();
    if s.is_empty() { return Some(TgCommand::SnoozeUntil { title: String::new(), time: None }); }
    let (title_out, time_out) = match s.rfind(char::is_whitespace) {
        Some(pos) => {
            let left = s[..pos].trim();
            let right = s[pos..].trim();
            if let Some(hm) = parse_sleep_until_time(right) {
                (left.to_string(), Some(hm))
            } else {
                (s.to_string(), None)
            }
        }
        None => (s.to_string(), None),
    };
    Some(TgCommand::SnoozeUntil { title: title_out, time: time_out })
}
```

- rsplit 末 whitespace token 作 HH:MM — 与 /pri / /promote 同 parser
  模板（title 含空格 / 中文标点都保）
- 复用 `parse_sleep_until_time` helper — 接受 HH:MM / H:MM / HH /
  H（单数字视为 HH:00）
- 解析失败 → time=None + 整段作 title；handler 走 usage hint

#### 3. `format_snooze_until_reply` pure 函数

4 态：
- title 空 → usage hint
- time=None → 「不是合法时刻」+ usage hint
- save_ok=Err → 显具体失败原因
- 成功 → 「💤 已 snooze『title』到 HH:MM」+ 跨日 hint（如适用）+
  `/unsnooze {title}` follow-up hint

### `src-tauri/src/telegram/bot.rs`

handler 紧贴 `SleepUntil`：

```rust
TgCommand::SnoozeUntil { title, time } => {
    if title.trim().is_empty() || time.is_none() {
        format_snooze_until_reply(&title, time, None, false, Ok(()))
    } else {
        use chrono::{Datelike, Local, TimeZone};
        let (h, m) = time.unwrap();
        let now = Local::now();
        let today_target = Local.with_ymd_and_hms(now.year(), now.month(), now.day(), h, m, 0).single();
        let target = match today_target {
            Some(t) if t > now => t,
            Some(t) => t + chrono::Duration::days(1),
            None => now + chrono::Duration::hours(1),
        };
        let crosses_midnight = today_target.map(|t| t <= now).unwrap_or(false);
        let until_str = target.format("%Y-%m-%d %H:%M").to_string();
        let resolved = match try_resolve_by_index(&title, chat_id.0, state).await {
            Some(t) => Ok(t),
            None => resolve_tg_task_title(&title),
        };
        let save_ok = match resolved {
            Ok(t) => task_set_snooze(t.clone(), Some(until_str)).map_err(|e| e.to_string()),
            Err(e) => Err(e),
        };
        format_snooze_until_reply(&title, Some((h, m)), Some(target), crosses_midnight, save_ok)
    }
}
```

- **跨日规则与 /sleep_until 完全一致**：目标 ≤ now → 明日同时刻；
  DST fallback now+1h
- **title resolve 3 层**：try_resolve_by_index（数字 index）→
  resolve_tg_task_title（fuzzy + exact）— 与 /snooze / /done / /cancel
  同模板
- **复用 task_set_snooze**：与桌面 PanelTasks 调期 popover / 既有
  /snooze 同后端 — `[snooze: YYYY-MM-DD HH:MM]` marker 一处写入点

### Registry + ALL_HELP_TOPICS + help-for-topic + table line + 2x drift defense

- 双 lang registry 各加一条（紧贴 sleep_until）
- ALL_HELP_TOPICS 紧贴 "sleep_until"
- format_help_for_topic 加详细文案 + /sleep_until / /snooze /
  /snoozed / /unsnooze 交叉引用
- format_help_text 全表加 `/snooze_until <title> <HH:MM>` 一行
- 两处 drift-defense 测试列表加 "snooze_until"

### 8 单元测试

- parse（title + time / empty / invalid-time-fall-into-title）× 3
- format（empty title / invalid time / save failure / success / cross
  midnight）× 5

## Key design decisions

- **rsplit 末 token 作 HH:MM 而非首 token**：title 含空格 / 中文是常态
  （"整理 Downloads"）— 末 token 解析与 /pri / /promote / /edit_due 同
  模板 owner 心智一致
- **time: Option<(u8, u8)> 而非 Result**：parse 失败时仍存 title 让
  handler 显「时刻不合法 + usage」；与 /sleep_until 同 graceful 模式
- **跨日规则与 /sleep_until 共享**：两命令都是「绝对 HH:MM 命令」，
  跨日语义一致（目标 ≤ now → 明日同时）— owner 学一次跨命令通用
- **复用 task_set_snooze 既有 backend**：snooze marker 一处真实施加
  点；既有 backend 已 production 验证含 strip 旧 marker + append 新
  marker + idempotent
- **不引 cancel snooze 路径**：清 snooze 走既有 /unsnooze 单命令；本
  命令仅"设到指定时刻"语义清晰
- **不写 unit test on async handler**：handler 是 stitching（title
  resolve + chrono 算 target + task_set_snooze + formatter wrap）；
  formatter 单测 + parse_sleep_until_time 单测 + task_set_snooze 既有
  tests 各 cover 主要逻辑。GOAL.md "meaningful tests only" 规则下不
  引装饰性 handler test

## Verification

- `cargo build --lib` — clean
- `cargo test --lib telegram::commands::tests::snooze_until` — 3/3 通过
- `cargo test --lib telegram::commands::tests::format_snooze_until` —
  5/5 通过
- `cargo test --lib`（全表）— 1563/1563 通过（+8 from 1555）
