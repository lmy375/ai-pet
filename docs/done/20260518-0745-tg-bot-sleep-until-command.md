# TG bot `/sleep_until <HH:MM>` 命令（iter #460）

## Background

TG bot 已有两条静音命令：
- `/mute [N]` — 相对分钟数（缺省 30；clamp 0..=10080）
- `/sleep` — 固定 8 小时晚安式静音

owner 想「安静到 8 点」/「安静到中午」/「安静到 22:30」时，要心算
「现在到目标时刻多少分钟」再走 /mute N — 心智成本高。本 iter 加
`/sleep_until <HH:MM>` 让 owner 直接说目标时刻。

## Changes

### `src-tauri/src/telegram/commands.rs`

#### 1. `TgCommand::SleepUntil { raw: String }` 变体

紧贴 `Mute`（同静音族）。raw 留给 handler 解析 + 算 minutes（不在 parser
里依赖 chrono，让 parser 保 pure）。

#### 2. 解析（parser）

```rust
"sleep_until" => Some(TgCommand::SleepUntil { raw: title }),
```

所有 trailing arg 当 raw 字符串（含空白保留）— 空 raw 由 handler 走
missing-arg。

#### 3. `parse_sleep_until_time` pure helper

```rust
pub fn parse_sleep_until_time(s: &str) -> Option<(u8, u8)> {
    let s = s.trim();
    if s.is_empty() { return None; }
    if let Some((h_str, m_str)) = s.split_once(':') {
        let h: u8 = h_str.trim().parse().ok()?;
        let m: u8 = m_str.trim().parse().ok()?;
        if h < 24 && m < 60 { Some((h, m)) } else { None }
    } else {
        let h: u8 = s.parse().ok()?;
        if h < 24 { Some((h, 0)) } else { None }
    }
}
```

接受格式：
- `HH:MM` / `H:MM`（标准 24h）
- `HH` / `H` 单数字（视为 HH:00 — owner 说"到 8 点 / 14 点"省冒号）
- 内部 trim 防边距 whitespace

拒绝：超 24h / >= 60min / 非数字 / 空。

#### 4. `format_sleep_until_reply` pure 函数

```rust
pub fn format_sleep_until_reply(
    raw_arg: &str,
    parsed_time: Option<(u8, u8)>,
    minutes: i64,
    until_local: Option<chrono::DateTime<chrono::Local>>,
    crosses_midnight: bool,
) -> String;
```

3 态：
- raw 空 → usage hint
- parse 失败 → 错误 hint 含原 raw（让 owner 看自己输入哪错了）
- 成功 → 「🌙 已静音 proactive 到 HH:MM（N 分钟 / 小时 / 天 后自动解除）」+
  `crosses_midnight` 时 append 「（明日同时刻 — 目标 ≤ now 自动跨日）」hint

复用 /mute reply 的 "N 分钟 / N 小时 X 分钟 / N 天 N 小时" 三级 nice
duration 格式让 owner 看得舒服。

### `src-tauri/src/telegram/bot.rs`

handler 紧贴 `Mute`：

```rust
TgCommand::SleepUntil { raw } => {
    let parsed = parse_sleep_until_time(&raw);
    match parsed {
        None => format_sleep_until_reply(&raw, None, 0, None, false),
        Some((h, m)) => {
            let now = Local::now();
            let today_target = Local.with_ymd_and_hms(now.year(), now.month(), now.day(), h, m, 0).single();
            let target = match today_target {
                Some(t) if t > now => t,
                Some(t) => t + chrono::Duration::days(1),  // 跨日
                None => now + chrono::Duration::hours(1),   // DST fallback
            };
            let crosses_midnight = today_target.map(|t| t <= now).unwrap_or(false);
            let minutes = (target - now).num_minutes().clamp(1, 10080);
            let _ = set_mute_minutes(minutes);
            let until_local = Some(target);
            format_sleep_until_reply(&raw, Some((h, m)), minutes, until_local, crosses_midnight)
        }
    }
}
```

设计：
- **跨日规则**：目标时刻 ≤ now → 落到明日同时刻（chrono `+ Duration::days(1)`）。
  owner 凌晨 1 点说「到 8 点」是「今早 8:00」语义（4h），不是「明日 8:00」
  （27h）反直觉。`crosses_midnight` flag 传 formatter 让 reply 加注 hint
  让 owner 知道发生了跨日
- **DST fallback**：`with_ymd_and_hms(...).single()` 在 DST 春进秋退当天
  目标时刻可能 None / Ambiguous — 极少出现但兜底 now+1h 让命令至少
  不哑火
- **clamp 1..=10080**：与 /mute 同 cap（≤ 7 天），handler 内 enforce
- **复用 `set_mute_minutes`**：与 /mute / /sleep / 桌面 PanelDebug "⚙️
  mute" 同后端 — 一处 mute 真实施加点；不引新 backend

### Registry + ALL_HELP_TOPICS + help-for-topic + table line + 2x drift defense

- 双 lang registry 各加一条（紧贴 mute）
- ALL_HELP_TOPICS 紧贴 "sleep"
- format_help_for_topic 加详细文案 + 交叉引用 /mute / /sleep；同步在
  /sleep 详细文案末追加 /sleep_until 交叉引用
- format_help_text 全表加 `/sleep_until <HH:MM>` 一行
- 两处 drift-defense 测试列表加 "sleep_until"

### 11 单元测试

- parser（HH:MM / 单数字 / 空 raw）× 2
- parse_sleep_until_time（HH:MM / 单数字 / 超范围拒 / 垃圾拒 / trim
  空白）× 5
- format_sleep_until_reply（空 raw / 无效 / 成功 / 跨日 hint）× 4

## Key design decisions

- **跨日 fallback 落明日而非"今日已过 = 错误"**：owner 心智「到 8 点」
  在凌晨说 = 今早 8:00；下午说 = 明早 8:00。reject 反而让 owner 失望
  「我说了到 8 点啊」。这条决策也与桌面 `dueTonight` / `dueTomorrow`
  helper 的"target ≤ now 自动 +1d"模式一致
- **`HH` 单数字接受**：owner 输 `/sleep_until 14` 显然意思 14:00；不
  接受会被 reject 是 friction。clamp h < 24 防 99 这种乱输
- **handler 算 minutes + 调 set_mute_minutes（不引新后端）**：本命令
  本质是「相对分钟数计算 helper + /mute 同后端」，不需要新 mute
  primitive
- **parser 不做时刻数学**：让 chrono / Local::now 留在 handler — 让
  parser 保 pure 易测；handler 是 IO 边界天然不 pure
- **不写 chrono-based unit test on handler**：handler 的"now → target
  → minutes"算术需要 mock 时钟（pet 代码库无 inject 时钟 pattern），
  与 parser / formatter 解耦后 handler 仅 stitching；formatter 单测
  覆盖 nice duration / cross-midnight hint 即够
- **`crosses_midnight` flag 传 formatter**：让 reply 区分「今日内」
  与「跨日」 — owner 看到「明日同时刻」hint 不会以为命令乱算

## Verification

- `cargo build --lib` — clean
- `cargo test --lib telegram::commands::tests::sleep_until` — 2/2
  通过
- `cargo test --lib telegram::commands::tests::parse_sleep_until_time`
  — 5/5 通过
- `cargo test --lib telegram::commands::tests::format_sleep_until`
  — 4/4 通过
- `cargo test --lib`（全表）— 1536 / 1536 通过（+11 from 1525）
