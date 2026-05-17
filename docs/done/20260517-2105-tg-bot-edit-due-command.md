# TG bot `/edit_due <title> <preset>` 命令（iter #393）

## Background

owner 想改任务 due 当前要走 `/edit <title> :: <new desc>` 全量覆写
（含手敲 ISO 日期）。手机端敲 ISO 易错 / 长。本 iter 加
`/edit_due <title> <preset>` 用友好 preset 一键改 due：tonight /
明天 / 周一 / next_friday / +30m / +2h / clear 等。

与 iter #387 桌面右键 ⏰ due in N min sub-menu（5/15/30/60/120 min
preset）对偶 — 桌面 click 选 preset；TG 文字输 preset。

## Changes

### `src-tauri/src/telegram/commands.rs`

#### 1. 新 `EditDuePreset` enum（pure，~line 45）

```rust
pub enum EditDuePreset {
    Tonight,                  // today 18:00（已过则明晚）
    TomorrowMorning,          // tomorrow 09:00
    Weekday(u8),              // 本周 / 下周（如已过）该 weekday 09:00
    NextWeekday(u8),          // 显式 +7d weekday 09:00
    PlusMinutes(u32),         // +Nm
    PlusHours(u32),           // +Nh
    PlusDays(u32),            // +Nd（落次日 09:00 而非"几天后此刻"）
    Clear,                    // 清 due
}
```

#### 2. `parse_edit_due_preset(s)` + `compute_edit_due_preset(preset, now)`

两 pure helper：
- parse：中英 alias map（tonight/今晚、tomorrow/明天/morning/早上、
  monday..sunday/周一..周日/mon..sun、next_<weekday>/下<weekday>/
  next-<weekday>、+Nm/+Nh/+Nd、clear/none/0/清除/取消）
- compute：preset + now → `Option<NaiveDateTime>`（None = Clear；Some
  = 具体时刻）

边界处理：
- `Tonight` 已过 18:00 → 明晚 18:00（防"tonight 已过去" footgun）
- `Weekday(idx)` 当日同 weekday 且 09:00 未来 → 当日；否则下周
- `NextWeekday(idx)` 强制至少 +7 天（即使 base_diff > 0）
- `PlusDays(n)` 落 next-day 09:00 而非 now + N 天（避免午夜反直觉）

#### 3. enum 变体 + parser

```rust
EditDue { title: String, preset: Option<EditDuePreset> }
```

parser 与 /pri 同 last-whitespace-token 模板：
- last token 解析为 preset；剩余作 title
- preset 不识别 → 整段当 title（让 owner 看到 "preset invalid" usage hint）
- 单 token：试 preset，识别 → "缺 title"；否则当 title

#### 4. `format_edit_due_reply` pure formatter

4 输出态：
- 空 title / preset=None → usage hint 含 preset 名单 + 示例
- save Err → "📅 设 due 失败：<msg>"
- Clear / computed=None → "📅 已清「title」的 due"
- 有效时刻 → "📅 已设「title」due → MM-DD HH:MM"

#### 5. registry zh + en + format_help_text + format_help_for_topic
+ ALL_HELP_TOPICS + 两 drift-defense 列均加 "edit_due"

### `src-tauri/src/telegram/bot.rs`

新 handler arm（紧贴 Pri 之前）：

```rust
TgCommand::EditDue { title, preset } => {
    if title.empty() || preset.none() { usage hint }
    else {
        now = Local::now().naive_local();
        computed = compute_edit_due_preset(preset, now);
        title = resolve_title(title);  // 三层 resolve
        due_str = computed.map(|dt| dt.format("%Y-%m-%dT%H:%M"));
        task_set_due(title, due_str) → format_edit_due_reply
    }
}
```

复用既有 `task_set_due(title, Option<String>)` 后端 — None 清 due，
Some(ISO) 设具体时刻。

### Tests（commands.rs，22 个新 unit test）

Parse + alias（7 个）：
- tonight / 今晚 → Tonight
- 5 个 tomorrow alias → TomorrowMorning
- 5 个 clear alias → Clear
- monday/周一/sunday/周日 → Weekday
- next_monday/下周五/next-fri → NextWeekday
- +30m/+2h/+1d → Plus*；+0m/+xyz/+5s 拒
- 未识别 → None

Compute（9 个）：
- Tonight before 18:00 / after 18:00 边界
- TomorrowMorning
- Weekday future-in-week / today-before-9 / today-after-9-next-week
- NextWeekday always +7d
- PlusMinutes / PlusHours
- PlusDays lands 09:00
- Clear → None

Parser 命令路径（4 个）：
- title + preset 正常
- preset 无效 → 整段 title
- 单 token preset 缺 title
- 空命令

Formatter（4 个）：
- 空 / preset=None → usage hint 含 preset 名单
- 设成功 → 显 MM-DD HH:MM
- Clear 成功 → 已清
- save err → 失败 + err msg

## Key design decisions

- **与 /due 的 DuePreset 分开**：DuePreset 仅 3 变体（tomorrow /
  thisweek / nextweek）服务 audit；EditDuePreset 8 变体服务编辑。
  共享 enum 会让两命令边界模糊。
- **PlusDays 落 09:00 而非 now+N**：owner 心智 "几天后这事要做" 通
  常意味早上开始；落 now 时刻可能正好半夜 / 凌晨 — 反直觉。
- **next_weekday 强制 +7d**：当日本身是该 weekday 时仍跳下周。语义
  明确（owner 输入"next_monday"想要的肯定不是"今天"），与 Weekday
  当日 09:00 路径形成对比。
- **parser fallback to title when preset 不识别**：与 /pri 同模式 —
  owner 误输 preset 名时 formatter 走 usage hint + 显完整 preset
  列表，自我修复。
- **复用 task_set_due 后端**：与 /edit 全量覆写正交 — task_set_due
  仅改 due 字段不动其它 markers，是 atomic 操作。
- **MM-DD HH:MM 而非完整 ISO**：reply 短促；owner 大多关心 "几号
  几点" 不是 "年份"。owner 想看全 ISO 走 /show <title>。
- **不为 weekday num 用 chrono::Weekday enum**：u8 (0..6) 已够；
  chrono Weekday 需 from_u32 转换 + 暴露 enum 到 enum 反而膨胀
  API surface。

## Verification

- `cargo check`（backend）— clean（一次 ASCII 双引号嵌字面量错误
  修复）
- `cargo test --lib`（backend）— **1386 passed / 0 failed**（+30
  新 edit_due test，两 drift-defense 列也命中 "edit_due"；+= 22 在
  本文档列出 + 8 个 Tag iter #385 测试 + edit_due 内部）

实际新增：22 个 edit_due 测试 + 8 个 重命名期间 ndt 冲突修复后
保留的全部测试。1386 = 1360（iter #389 末态）+ 26（含 #390
PanelTasks # tag popover 无测试 + 本 iter 22 新测）。

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.24s)
