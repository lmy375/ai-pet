# TG bot 加 `/alarms_today` 命令（iter #528）

## Background

`/alarms [N]` 列最近 N 条 pending reminders（按 target asc）— 不限日
期。但常用场景是「今天还会响哪些 alarm」/「哪些已逾期未消」 — owner
要回答这些需要 /alarms 的输出再心算「这条是今日还是明日」。

本 iter 加 `/alarms_today` — 同 backend 数据源 + formatter 过滤到今日
target。无 N 参（今日范围天然小，cap 反而误导）。

## Changes

### `src-tauri/src/telegram/commands.rs`

按 6+ surface 模式同步：

1. **Enum 变体** `AlarmsToday`（无参，紧贴 SearchYesterday 之后）
2. **`name()` arm** → `"alarms_today"`
3. **`title()` arm** → 加入无参 arm 集
4. **parser arm** `"alarms_today" => Some(TgCommand::AlarmsToday)` —
   多余尾部一律忽略
5. **en + zh registry** entries
6. **`ALL_HELP_TOPICS`** 加 `"alarms_today"`
7. **`format_help_for_topic("alarms_today")`** 详细文案（含与 /alarms /
   /touched_today / /today 关系）
8. **`format_help_text`** 表格行
9. **两份 drift-defense test 列表**

#### 纯 formatter `format_alarms_today_reply`

```rust
pub fn format_alarms_today_reply(
    rows: &[(ReminderTarget, String, String)],
    now: chrono::NaiveDateTime,
) -> String {
    let today = now.date();
    let filtered: Vec<_> = rows.iter().filter(|(target, _, _)| match target {
        ReminderTarget::TodayHour(_, _) => true,                    // 按定义今日
        ReminderTarget::Absolute(dt) => dt.date() == today,         // 比对日期
    }).collect();
    if filtered.is_empty() {
        return format!("⏰ 今日（{}）暂无 alarm。\n用 /alarms 看不限日期...");
    }
    // 行格式：「· HH:MM (剩 N 分 / 已逾期 N 分) | <topic>」
    // header 含 date — 行内 HH:MM only（avoid MM-DD 冗余）
    ...
}
```

与 `format_alarms_reply` 差异点：

- filter 到 target.date() == today
- 无 N cap（今日小集合 + 「漏看」比「滚屏」糟）
- header 用「今日（DATE）N 条」而非「最近 N 条 pending」
- 行 HH:MM only（date 已在 header）
- 空集教学指 /alarms — 避免 loop

### `src-tauri/src/telegram/bot.rs`

Handler 紧贴 /alarms 之前 — 同 read path（todos_as_memory_items +
parse_reminder_prefix）+ 同 sort（target asc），仅 formatter 调
`format_alarms_today_reply`。

## Key design decisions

- **无 N param**：今日范围天然小（典型 < 10 条 reminders）；cap 反而
  漏看（owner 心智 "/alarms_today 看今天" → 期望完整列表）。与
  /touched_today 同设计 — 「时段切片」类命令不需 N
- **header 含 date**：让 owner 看到 reply 时确认 scope 准确（午夜后跨
  日时尤其重要）
- **TodayHour 永远算今日**：按定义如此 — `[remind: 18:00]` 协议是
  「今日 18:00」（fire 后不重复触发）
- **空集兜底不指 /alarms_today 自身**：避免 loop；指 /alarms 让 owner
  扩 scope 看是否有未来日 alarm
- **复用 /alarms 的剩余/逾期分级算法**：分 < 60 分 / < 24 小时 / >= 24
  小时三档 — 心智一致；天 case 防御性保留（今日 target delta ≤ 24h，
  通常不触发）
- **5 个 unit tests pin 真实行为**（满足 GOAL.md "meaningful tests
  only"）：parser 1 + formatter 4（空集教学 / today 过滤 / HH:MM only
  header+line / 剩余 vs 逾期分级）

## Verification

- `cargo build`（src-tauri）— clean
- `cargo test --lib` — all 1662 tests pass（新 5 + 既有 1657）
- 三个 drift-defense test all pass
- 手测：
  - 今日有 alarm → 「⏰ 今日（DATE）N 条 alarms」+ list
  - 今日无 alarm → 友好兜底指 /alarms
  - TodayHour vs 今日 Absolute target 都包含；明日 Absolute 排除

## Future iters (out of scope)

- `/alarms_yesterday` — 昨日已 fire 的 reminder audit；按需 propose
- `/alarms_this_week` — 周维度；当前 daily 足够
- ChatMini ambient row 「⏰ 今日还 N 个 alarm」chip — 既有 `ambientAlarms`
  chip 已显总数；今日 scope 视角后续可考虑
