# TG bot `/snooze` + `/unsnooze` 命令

## 背景

TODO（auto-proposed 之前）：

> TG bot `/snooze <title> <duration>` 命令：与桌面右键 Snooze 对偶，手机端也能延后任务。

`task_set_snooze` Tauri 命令 + 桌面右键 4 预设（20260514-1318）+ TG `/done` `/cancel` `/retry` 都到位了，但 TG 端唯独缺 snooze 入口 —— 手机用户想"先放着，下周再说"只能切桌面。本轮补这条命令，让 TG 任务动词组（done / cancel / retry / snooze / unsnooze）完整。

## 改动（backend Rust only）

### `src-tauri/src/telegram/commands.rs`

**1. 新枚举 + 名字 / 标题路由**

```rust
TgCommand::Snooze { title: String, token: String },
TgCommand::Unsnooze { title: String },
```

token 在 parser 层只剥不解析（保 pure parse），handler 用 now 统一解析。`name() / title()` 各 match arm 同 done / cancel / retry 模式补全。

**2. parser 智能尾 token 剥离**

```rust
fn split_trailing_snooze_token(arg: &str) -> (String, String) {
    let words: Vec<&str> = arg.split_whitespace().collect();
    if words.len() < 2 { return (arg.to_string(), String::new()); }
    let last = words[words.len() - 1];
    if parse_snooze_token(last).is_some() {
        (words[..words.len() - 1].join(" "), last.to_string())
    } else {
        (arg.to_string(), String::new())
    }
}
```

仅当末 token 命中 preset 时剥下；不命中 → 整段当 title。**单 token 不剥**：`/snooze 30m` 视作 title `30m`（让 missing-argument 语义生效，而非"暂停 30m 但没 title"）。

**3. 纯 helper `parse_snooze_token` + `compute_snooze_until`**

```rust
pub enum SnoozeSpec {
    Minutes(u32),  // <N>m  · 1..=10080（≤ 7 天）
    Hours(u32),    // <N>h  · 1..=168（≤ 7 天）
    Tonight,       // 今晚 18:00（已过 → 明晚）
    Tomorrow,      // 明天 09:00
    Monday,        // 下个周一 09:00（今日是周一也跳下周一）
}

pub fn parse_snooze_token(token: &str) -> Option<SnoozeSpec>;
pub fn compute_snooze_until(spec: SnoozeSpec, now: NaiveDateTime) -> NaiveDateTime;
```

边界与桌面 PanelTasks 右键 Snooze chip 完全一致 —— "下周一"在今日是周一时跳 +7 天（"下周一" = 下周第一天的语义稳定）。`tonight` 已过 18:00 时跳 tomorrow 18:00（防"点了反而退到过去"）。

**4. parser case**

```rust
"snooze" => {
    let (title, token) = split_trailing_snooze_token(&title);
    Some(TgCommand::Snooze { title, token })
}
"unsnooze" => Some(TgCommand::Unsnooze { title }),
```

**5. tg_command_registry 注册（zh / en 各加 2 条）**

```rust
("snooze", "暂停任务（30m / 2h / tonight / tomorrow / monday，缺省 30m）"),
("unsnooze", "解除任务暂停"),
```

让 TG 的 slash autocomplete 列表能浮出来。

**6. format_help_text 加一行**

```
/snooze <title> [preset] | /unsnooze <title>  —  暂停 / 解除暂停（preset = 30m / 2h / tonight / tomorrow / monday）
```

### `src-tauri/src/telegram/bot.rs`

**1. 空 title 守门** —— `Snooze { ref title, .. }` 和 `Unsnooze { ref title }` 加入 `missing_argument` 联合 match。

**2. 两个 dispatch handler**

Snooze：
```rust
let spec_result: Result<SnoozeSpec, String> = if token.is_empty() {
    Ok(SnoozeSpec::Minutes(30))  // 默认 30m
} else {
    parse_snooze_token(&token).ok_or_else(|| format!("未知 preset「{}」 — 支持 ...", token))
};
match spec_result {
    Err(msg) => format_command_error(&msg),
    Ok(spec) => {
        let actual = match try_resolve_by_index(...).await { Some(t) => Ok(t), None => resolve_tg_task_title(&title) };
        match actual {
            Ok(t) => {
                let now = chrono::Local::now().naive_local();
                let until = compute_snooze_until(spec, now);
                let until_str = until.format("%Y-%m-%d %H:%M").to_string();
                match task_set_snooze(t.clone(), Some(until_str.clone())) {
                    Ok(()) => format!("💤 已暂停「{}」至 {}\n如需解除发 /unsnooze {}", t, until_str, t),
                    Err(e) => format_command_error(&e),
                }
            }
            Err(msg) => format_command_error(&msg),
        }
    }
}
```

**先校验 token 再 resolve title** —— 让 invalid-token 错误比 task-not-found 优先级高（用户先解决 typo，再考虑 title 是否对）。

Unsnooze：简单 resolve + `task_set_snooze(t, None)` → `format!("☀️ 已解除「{}」 的暂停", t)`。

复用既有 `try_resolve_by_index` + `resolve_tg_task_title` 三层（数字 index → fuzzy → 错误），与 /done /cancel /retry 体感对齐。

### 测试

**16 个新单测**（commands.rs 内）：

- `parses_snooze_with_preset_token` / `parses_snooze_no_preset_token` / `parses_snooze_single_word_arg_is_title_not_preset` / `parses_snooze_minutes_form` / `parses_unsnooze`：parser 5 个
- `parse_snooze_token_keywords` / `parse_snooze_token_minutes_hours` / `parse_snooze_token_rejects_invalid`：token 3 个
- `compute_snooze_until_minutes` / `hours` / `tonight_before_6pm` / `tonight_after_6pm_jumps_tomorrow` / `tomorrow` / `monday_when_today_is_monday_jumps_next_week` / `monday_when_today_is_wednesday` / `monday_when_today_is_sunday`：compute 8 个

## 不做

- **不支持 `YYYY-MM-DD HH:MM` 原始时间格式**。手机用户在 TG 里手敲 ISO 不现实；preset 已覆盖 95% 用例。要绝对时刻可走桌面右键菜单或 LLM `butler_task_edit`。
- **不让 LLM 调用 /snooze**。和其它 TG 命令一样，是用户面向命令；LLM 直接走 `butler_task_edit` 改 description。
- **不动 /tasks 列表显示 snooze 状态**。当前 list 已能反映任务状态；snooze 仅影响 proactive 选单（filter）+ panel 💤 chip，TG 列表的视觉补 snooze chip 是独立改动。
- **不缓存 spec 解析**。每次都重新算，逻辑廉价。

## 验证

- `cargo check` ✓ 0 error
- `cargo test --lib telegram::` ✓ 184 / 184 通过（168 → 184，net +16 snooze 测试）
- `cargo test --lib` ✓ **978 / 978 通过**（962 → 978）

## 后续

- TG /tasks 列表对 snooze 任务加 💤 chip / 段（与桌面 PanelTasks 视觉对偶）。
- 自然语言派单时 LLM 识别"先放一周"语义自动加 `[snooze: ...]` —— 当前需 user 显式 `/snooze`，未来 LLM 主动加 marker 更顺。
- `/whoami` 命令也加进 tg_command_registry（与本轮发现的小遗漏并存，未阻塞本次）。
