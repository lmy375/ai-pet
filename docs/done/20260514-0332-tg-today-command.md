# TG bot `/today` 命令 — 手机端今日叙事视图

## 背景

桌面 PanelChat 已有 `/today`（今日到期 / 今日完成的标题清单）。TG 端没有。`/stats` 给数字，`/today` 给清单，二者互补。手机用户在通勤 / 睡前等"非桌面时刻"最想问"今天还有啥要做" —— `/today` 比 `/tasks` 全量列表更聚焦。

## 改动

### `src/telegram/commands.rs`

- `TgCommand::Today` variant
- `name()` / `title()` 接上（无参，返回 ""）
- parser 加 `"today" => Some(TgCommand::Today)`，多余尾部忽略
- registry zh/en 各加一行
- 新 pure fn `format_today_reply(views: &[TaskView], today: NaiveDate) -> String`：
  - 桶分 `dueToday`（status==Pending + due 日期是 today）/ `doneToday`（status==Done + updated_at.starts_with(today_str)）
  - 输出格式与桌面 `/today` 一致：`📅 今日（YYYY-MM-DD）\n\n今日到期（N）：\n· …\n\n今日已完成（M）：\n· …`
  - 每段 cap 5 + `…还有 K 条` 溢出
  - 全空 → "📅 今日（YYYY-MM-DD）\n\n今日队列清爽 ✨"
- `format_help_text` 加一行 `/today  —  今日叙事`
- 单测：parse hit / trailing-ignore / format empty / format mixed

### `src/telegram/bot.rs`

`TgCommand::Today` handler：

```rust
let views = read_tg_chat_task_views(chat_id.0);
let today = chrono::Local::now().date_naive();
crate::telegram::commands::format_today_reply(&views, today)
```

reuse 既有 `read_tg_chat_task_views`（与 /tasks / /stats 同 read path）。

## 不做

- 不分 due/done 之外的 status 段：那是 /stats 数字汇总的活
- 不显 HH:MM 时间后缀（与桌面 /today 略不同）：TG 端单行字数预算紧，cap 5 已经够 LCD；title 自己够区分
- 不抽公共 helper：与 format_stats_reply 不同 origin filter / 桶逻辑；都 < 50 行各自清晰

## 验收

- `cargo build --release` ✅
- `cargo test --lib` ✅（含 4 新测试）
- TG 发 `/today` → 两段标题清单或"今日队列清爽 ✨"
- `/help` 输出含 `/today` 一行

## 完成

- [x] commands.rs: enum + parser + registry + format_today_reply + 5 测试
- [x] bot.rs: handler
- [x] format_help_text 加一行
- [x] `cargo build --release` 通过
- [x] `cargo test --lib` 通过（912 passed，+5 新）
- [x] README 一行
- [x] 移到 docs/done/
