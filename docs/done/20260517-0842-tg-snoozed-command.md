# TG bot `/snoozed` 命令（iter #271）

## Background

TG bot 已有 `/pinned` / `/silenced` 命令查 owner-intent markers，但缺少
`[snooze: ...]` 维度。owner 在外面想 audit "我哪些任务被暂存了 / 还多久回
到队列" 目前没专门入口（要 `/tasks` 看完整列表，无 snooze 信号显示）。

本迭代加 `/snoozed`：列本 chat 派单中 `snoozed_until` 仍未过点的 task，按
醒来时刻升序（最近醒的在前），每行显倒计时（N 分 / N 时 / N 天 后醒）+ 状
态 emoji + 精确醒时刻（MM-DD HH:MM）。

## Changes

仅后端：

- `src-tauri/src/telegram/commands.rs`：
  - `TgCommand::Snoozed` 新变体（无参）
  - `parse_tg_command` 加 `"snoozed"` 分支（多余尾部忽略）
  - registry zh+en 注册
  - `format_snoozed_reply(views, now)`：
    - filter views with parseable `snoozed_until`
    - sort by until asc（最近醒的在前）
    - 倒计时 label：< 60 min → `N 分后醒`；< 24h → `N 时 / N 时 M 分后醒`；
      ≥ 24h → `N 天 / N 天 H 时后醒`
    - 行格式：`emoji title · 倒计时（MM-DD HH:MM）`
    - 空 → 友好引导文案含 `/snooze` 用法示例
  - help text 加一行
  - 7 个新单元测试覆盖：parse / 空 friendly + command hint /
    无 snoozed_until 跳过 / 分钟 label / 时分 label / 天时 label / 按醒时间升序

- `src-tauri/src/telegram/bot.rs`：`TgCommand::Snoozed` 分支调
  `format_snoozed_reply`；views 走既有 `read_tg_chat_task_views(chat_id)` +
  `filter(|v| v.snoozed_until.is_some())`（backend build_task_view 已做
  active-only `now < until` 填充）。

## Key design decisions

- **依赖 TaskView.snoozed_until 而不是 raw_description regex 解析**：build_task_view
  已实现 `parse_snooze` + active-only 过滤 + 写入此字段；formatter 不需重做。
- **按 wake time asc 而不是 desc**：owner 在 TG 上关心"下一个回到队列的是
  哪条"（即将醒的最重要）；后醒的可以晚点再 check。
- **倒计时三级 label**：minute / hour / day 三档 + 跨档加 minor 单位（"2 时 30 分"
  / "3 天 5 时"），让 owner 不必心算"还要等多久"。
- **附 MM-DD HH:MM 绝对时刻**：相对倒计时 + 绝对时间双显，前者给"还多久"
  直觉，后者给"是哪个时刻"精确（防 timezone 困惑 / 日期边界）。
- **`now` 参数注入**：formatter 是 pure；caller 在 bot.rs 拿
  `chrono::Local::now().naive_local()`，测试传固定时刻让所有 label 断言可重复。

## Verification

- `cargo check` ✅
- `cargo test`（含 7 新测试 + 全表 1064 通过）✅
- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.21s)
