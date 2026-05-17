# TG bot `/today_done` 命令（iter #409）

## Background

既有 `/today` 给 owner「今日完整叙事」— 含 due 段（今日 due 的
pending）+ done 段（今日已 done 标题）。但 done 段没有 result 摘要 —
owner 想"我今天做完啥 + 各条产物"扫读时需切到 `/digest [N]` 看最近
N done（不限日期）+ result。

`/yesterday` 是「昨日 done + result」一行式，已是 owner 想要的格式
但 scope 是昨天。本 iter 加 `/today_done` 镜像今日 scope —— 与
/yesterday 同模板，让"昨日 audit + 今日复盘"两端对称。

## Changes

### `src-tauri/src/telegram/commands.rs`

#### 1. `TgCommand::TodayDone` 变体（无参）

紧贴 `Yesterday`，加到 name() / title() 空字符串两个 arm 集合中。
TG 命令名 `today_done` snake_case（dash 被 drift-defense parser
拒绝）。

#### 2. 解析

```rust
"today_done" => Some(TgCommand::TodayDone),
```

`parse_tg_command` 已经 lowercase 处理 head，所以 `/TODAY_DONE` /
`/today_done now` 等变种统一命中。

#### 3. `format_today_done_reply(views, today)` pure 函数

```rust
pub fn format_today_done_reply(views, today) -> String {
    let t_str = today.format("%Y-%m-%d").to_string();
    let mut done: Vec<&TaskView> = views.iter()
        .filter(|v| matches!(v.status, TaskStatus::Done))
        .filter(|v| v.updated_at.starts_with(&t_str))
        .collect();
    if done.is_empty() {
        return "📅 今日（{t_str}）暂无完成记录。\n用 /today 看今日 due / /yesterday 看昨日产出。";
    }
    done.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    // 每条 "· ✅ <title> — <result preview 40 chars>" 
}
```

设计：
- **clone /yesterday 实现而非抽 generic boundary fn**：保两 fn 独
  立单测点稳定 + 兜底文案语义不同（empty 时建议不同 alt 入口）
- **40 char result cap**：与 /yesterday 同 cap，reply 紧凑
- **updated_at ISO 前缀匹配 today_str**：`starts_with("2026-05-17")`，
  无需 chrono date parsing 容错性更好（异常 ts 直接 skip）
- **sort by updated_at desc**：最新 done 在前 — owner 习惯倒序扫读

#### 4. Registry + ALL_HELP_TOPICS + help-for-topic + format_help_text

- `tg_command_registry_localized` 两 lang 加 `("today_done", "...")`
- `ALL_HELP_TOPICS` 加 `"today_done"`
- `format_help_for_topic` 加 `"today_done" => "📅 ..."` 长文案（含
  与 /today / /yesterday / /digest / /streak 对比）
- `format_help_text` 全表加 `/today_done — ...` 一行
- /yesterday help 文案追加交叉引用 `/today_done`

#### 5. 两处 drift-defense 测试列表加 `"today_done"`

`tg_command_registry_covers_all_user_facing_commands` +
`format_help_for_each_listed_command_returns_detail` 同步。

### `src-tauri/src/telegram/bot.rs`

handler 紧贴 `Yesterday`：

```rust
TgCommand::TodayDone => {
    let views = read_tg_chat_task_views(chat_id.0);
    let today = chrono::Local::now().date_naive();
    format_today_done_reply(&views, today)
}
```

## Key design decisions

- **命名 `today_done` 而非 `done_today`**：与 /today 系命名族对齐
  （前缀 today_*），让 TG slash autocomplete 时邻位出现 /today /
  /today_done 视觉成组
- **不带 N 参数**：与 /today / /yesterday 同模板（无参） — 今日切
  片自然限定数量，不必 paginate
- **不含 archive 段**：本命令是 active 任务的 done 切片；archive
  task 不在 read_tg_chat_task_views scope 内（与 /today / /yesterday
  一致）
- **40-char result preview**：太短看不出产物本质，太长 TG 4096 字
  符限制下条数受限；与 /yesterday / /digest 同 cap 保一致体验
- **7 条单测覆盖**：parse / empty fallback / status+date filter /
  sort desc / result rendering / long-result truncate / empty result
  omit — 与 /yesterday 同测试矩阵保 drift 检测

## Verification

- `cargo test --lib telegram::commands::tests::today_done` — 7 / 7 通过
- `cargo test --lib`（全表回归）— 1431 / 1431 通过
- `npx tsc --noEmit`（frontend）— clean（无变更）
- `npx vite build`（frontend）— clean
