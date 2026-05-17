# TG bot `/alarms [N]` 命令（iter #373）

## Background

桌面 iter #372 给 PanelMemory item 加 ⏰ alarm chip，创建 `todo` 类
目带 `[remind: ...]` 协议条目。但手机端 owner 想 audit "我设了哪些
alarm，何时到点" 无入口 — 需回桌面看 PanelMemory todo 段。

本 iter 加 TG `/alarms [N]` 让手机端一键列 todo 段 pending reminders，
含目标时刻 + 剩余分钟（或已逾期分钟）。完成 iter #372 桌面 ↔ TG
对偶。

## Changes

### `src-tauri/src/telegram/commands.rs`

#### 1. enum 变体（~line 185）

```rust
Alarms { n: u32 },
```

#### 2. `name()` / `title()` arms

- name → "alarms"
- title() → "" (N-only 簇)

#### 3. parser（~line 1037）

与 /digest / /feedback_history 同 clamp 模板 — N 缺省 5，clamp 1..=20。

#### 4. `format_alarms_reply(rows, now, n)` pure formatter

入参 `rows: &[(ReminderTarget, topic, title)]`（caller 按 target 升
序排好）+ `now: NaiveDateTime`（让 formatter 算 delta 保 pure）：

- 空 rows → 友好兜底 + 引导 PanelMemory ⏰ chip / [remind:] 协议
- 非空 → header `⏰ 最近 N 条 pending alarms：` + 逐行
  `· MM-DD HH:MM (剩 N 分 / 已逾期 N 分) | <topic>`
- 超 N 时 overflow hint "还有 X 条更晚 alarms"

remaining/overdue 三分桶（< 60min 分钟级 / < 24h 小时级 / ≥ 24h 天
级）— 与 PanelTasks `formatDueRelative` 同心智。

新 helper `format_target_short(target) -> String`：
- TodayHour → `HH:MM` 紧凑
- Absolute → `MM-DD HH:MM` 含日期（list 内区分日期值钱）

#### 5. registry zh + en + format_help_text + format_help_for_topic
+ ALL_HELP_TOPICS + 两 drift-defense 列均加 "alarms"

### `src-tauri/src/telegram/bot.rs`

新 handler arm（紧贴 SilentAll 之前）：

```rust
TgCommand::Alarms { n } => {
    let items = db::todos_as_memory_items();
    let mut rows = items.iter().filter_map(|item|
        parse_reminder_prefix(&item.description).map(|(t, topic)|
            (t, topic, item.title.clone()))
    ).collect();
    let now = Local::now().naive_local();
    rows.sort_by_key(|(t, _, _)| absolute_target(t, now));
    format_alarms_reply(&rows, now, n)
}
```

复用：
- `db::todos_as_memory_items()` 读 todo 类目（已存在）
- `proactive::parse_reminder_prefix` 解析 [remind: ...] 协议
- `proactive::ReminderTarget` 公开类型（pub use self::reminders::*）

### Tests（commands.rs，10 个新 unit test）

Parser（4 个）：
- 默认 N=5 / 显式 N=10 / clamp 上下界 / 非数字 fallback

Formatter（6 个）：
- 空 rows → bootstrap hint（含创建路径 + 协议说明）
- 未来 alarm → "剩 N 分" + topic 显
- 过去 alarm → "已逾期 N 分"
- 小时 / 天分桶（4h → 剩 4 小时 / 3 天）
- N cap + overflow hint
- TodayHour target（90 min → 剩 1 小时 bucket）

## Key design decisions

- **复用 proactive::reminders 模块 + ReminderTarget 类型**：avoid
  duplication — 同协议同语义。仅前端 / TG 端文案 layer 不同。
- **入参 `(target, topic, title)` 三元组 + now 注入 → 保 pure
  formatter**：与 /feedback_history reply 模式一致（caller 负责
  sort / 排序 / cap，formatter 仅渲染）。让 unit test 不需 mock
  time / file IO。
- **MM-DD HH:MM 而非仅 HH:MM**：跨日 alarms list 时区分关键。今天
  的 alarm 显日期略冗余但保格式统一，比"前 3 条仅 HH:MM、后续含
  日期" 这种分支噪音少。
- **"已逾期" 也显**：不过滤 stale entries — owner 想知道"哪些被错
  过了 / 该手动 ack" audit 价值高。proactive cycle 内部仍有
  is_reminder_due(window=30min) 控制实际 fire 触发；本命令仅列。
- **不 expose `due_now: bool`**：已用"剩 N 分 / 已逾期 N 分"含分
  钟级信息，owner 比 boolean 更能判断紧迫度。
- **不与 /tasks / /digest 共享 sort helper**：alarms 排序规则
  （target 升序 + TodayHour 解释为今日 HH:MM）专属本场景，与 task
  views sort 不同。

## Verification

- `cargo check`（backend）— clean
- `cargo test --lib`（backend）— **1318 passed / 0 failed**（+10
  新 alarms test，两 drift-defense 列也命中 "alarms"）
- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.25s)
