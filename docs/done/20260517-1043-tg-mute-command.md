# TG bot `/mute [N]` 命令（iter #282）

## Background

owner 在桌面侧已有 PanelDebug "⚙️ mute 15min" 按钮 / pet ctx menu / TG 端
`/sleep` 路径都能静音 proactive — 但 TG 端 `/mute` 入口缺失。"嘿宠物先安
静半小时"是 owner 在外面常用的请求，目前要走 set_mute_minutes 桌面侧或
PanelDebug ⚙️ mute 按钮。

本迭代加 `/mute [N]`：调既有 `proactive::set_mute_minutes(N)` — 与桌面同
后端，副作用一致（包括 `mute_count::record_mute_engaged` hook 让"🔕 今日
mute" chip 计数同步）。

## Changes

仅后端：

- `src-tauri/src/telegram/commands.rs`：
  - `TgCommand::Mute { minutes: i64 }` 新变体；`name() / title()` 同步
  - `parse_tg_command` 加 `"mute"` 分支：N 缺省 30，clamp `0..=10080`（≤ 7
    天），非数字尾部 fallback 默认 30
  - registry zh+en 注册（在 /snoozed 之后）
  - `format_mute_reply(minutes, until_local)` pure 函数：
    - `minutes <= 0` → "🔊 已解除静音"
    - 否则 → "🔕 已静音 proactive {N 分/时/天} （到 HH:MM 自动解除）"
    - until_local 由 caller 注入便于单测（pure 不读时钟）
    - 倒计时 label：< 60 → "N 分钟"；< 24h → "N 小时 M 分钟"；≥ 24h → "N 天 H 小时"
  - help text 多加一行
  - 8 个新单测覆盖：parse default / explicit / clamp 边界 / 非数字 fallback /
    format 解除 / 分钟 label / 时分 label / 天 label

- `src-tauri/src/telegram/bot.rs`：`Mute { minutes }` 分支：
  - 调 `crate::proactive::set_mute_minutes(minutes)` 真正写后端 MUTE_UNTIL
    （顺带触发 mute_count hook）
  - 拼 `until_local = Some(now + N min)` 或 `None`
  - 调 `format_mute_reply` 返反馈

- `src-tauri/src/telegram/commands.rs` test mod：`use chrono::TimeZone`
  让新 format 测试能用 `Local.with_ymd_and_hms`。

## Key design decisions

- **复用 proactive::set_mute_minutes**：桌面 / TG 走同后端 = 状态一致 +
  mute_count tracking 自动同步（owner 通过 TG /mute 也会被 "🔕 今日 mute"
  chip 计入）。
- **clamp 0..=10080**：与 set_mute_minutes 内部安全范围一致；7 天上限
  防 owner 误输 `/mute 99999` 把宠物静音半月。
- **N == 0 = 解除**：与 set_mute_minutes(minutes <= 0) 行为一致；让"嘿
  宠物醒醒"也一句话搞定。
- **pure formatter + caller 注入 until_local**：format_mute_reply 是 pure
  函数，单测无依赖时钟；生产路径 caller 在 bot.rs 拿 chrono::Local::now()
  + Duration::minutes(N) 算出绝对时刻传入。

## Verification

- `cargo check` ✅
- `cargo test`（含 8 新 mute 测试 + 全表 1085 通过）✅
- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.28s)
