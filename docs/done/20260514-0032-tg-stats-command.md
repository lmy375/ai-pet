# TG bot `/stats` 命令 — 当前 chat 任务状态速览

## 背景

TG bot 已有 `/task /tasks /done /cancel /retry /help`。但**没有一条"汇总数字"的命令** —— 手机上想问"目前我派出去的任务有几条待办 / 几条今天做完 / 有没有逾期"必须先 `/tasks` 翻完整列表然后人肉数。

`/stats` 一行回 5 个数字（pending / overdue / done-today / error / cancelled-today），是 TG 端最低成本的"对账"入口。

## 改动

### `src/telegram/commands.rs`

- `TgCommand` enum 加 `Stats` variant（无字段）
- `name()` 加 `Stats => "stats"`；`title()` 加 `Stats => ""`
- `parse_tg_command` 加 `"stats" => Some(TgCommand::Stats)`
- `tg_command_registry_localized` zh/en 各加一行
- 新 pure fn `format_stats_reply(views: &[TaskView], now: NaiveDateTime, today: NaiveDate) -> String`：
  - 遍历 views：状态分桶；done / cancelled 仅当 `updated_at.starts_with(today_str)` 时计入"今日"
  - pending 行的 `due` 解析（"YYYY-MM-DDTHH:MM" 格式）→ 与 now 比较得 overdue 计数
  - 输出 6 行（含 header）："📊 任务状态" / "○ 待办：N" / "🔴 逾期：N" / "✓ 今日完成：N" / "⚠️ 出错：N" / "🗑 今日取消：N"
  - 全 0 时 header 下追加 "（今日很安静 ✨）"
- 单测：
  - 解析：`parses_stats` 验证 `/stats` 命中 `TgCommand::Stats`
  - 注册表：扩 `tg_command_registry_covers_all_user_facing_commands`
  - format：构 5 条混合状态 view → 比对输出
  - format 空数组 → "今日很安静"

### `src/telegram/bot.rs`

- `TgCommand::Stats` handler：与 `format_tasks_for_chat` 同模式 —— 拉 `memory_list("butler_tasks")` → 过滤 origin==Tg(chat_id) → `build_task_view` → 调 `format_stats_reply(&views, now, today)` 返回 reply 文本
- missing-argument or-pattern 不变（stats 无参数，不在该分支）

### `format_help_text`

把 stats 加进 help 文案的命令列表行（保持简洁，与 `/tasks` 同 section）。

## 不做

- 不在 stats 里 inline 列任务标题：那是 `/tasks` 的活；stats 只是数字
- 不分 priority bucket：5 个状态数字够稠密，再分 priority 会让单条 TG 消息过长
- 不缓存 stats response：与 /tasks 的去重缓存不同 —— stats 计算极便宜，且用户反复查 stats 就是想看"现在到底什么样"，缓存反而误导

## 验收

- `cargo build --release` ✅
- `cargo test --lib` ✅ 全部通过（含新测试）
- TG 发 `/stats` → 收到 6 行数字摘要
- `/help` 命令清单含 stats 一行
- 已知 origin 是 panel 创建的任务**不**纳入计数（仅本 chat 的 TG-派出任务）

## 完成

- [x] commands.rs: enum + parser + registry + format + 测试
- [x] bot.rs: handler + 抽 read_tg_chat_task_views 共享 read path
- [x] format_help_text 补 stats
- [x] `cargo build --release` 通过
- [x] `cargo test --lib` 通过（895 passed）
- [x] README 提一行
- [x] 移到 docs/done/
