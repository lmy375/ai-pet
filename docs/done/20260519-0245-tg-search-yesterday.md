# TG bot 加 `/search_yesterday <kw>` 命令（iter #525）

## Background

iter #523 加 `/search_today` 后 fuzzy-search 矩阵 today 行已完整。今日
矩阵：

|                   | 不限日期       | 限今日           | 限昨日 ← new       |
|-------------------|----------------|------------------|--------------------|
| 无 kw             | /tasks         | /touched_today   | /touched_yesterday |
| 含 kw             | /find          | /search_today    | **/search_yesterday** |

本 iter 补昨日 + kw 交集 — 早会回顾「昨天处理过的 X 相关 task」/ 周一
回顾「上周五碰过的 deploy issue」场景。

## Changes

### `src-tauri/src/telegram/commands.rs`

按 6+ surface 模式同步：

1. **Enum 变体** `SearchYesterday { keyword: String }`（紧贴 SearchToday）
2. **`name()` arm** → `"search_yesterday"`
3. **`title()` arm** → 与 SearchToday / Find / FindInDetail 同 keyword 列
4. **parser arm**：与 /search_today / /find 同 single-arg 模板
5. **en + zh registry** entries
6. **`ALL_HELP_TOPICS`** 加 `"search_yesterday"`
7. **`format_help_for_topic("search_yesterday")`** 详细文案（含矩阵 +
   alt 入口 + 跨周末取周日的注解）
8. **`format_help_text`** 表格行
9. **两份 drift-defense test 列表**

#### 纯 formatter `format_search_yesterday_reply`

clone `format_search_today_reply` 结构（filter / status rank / cap /
emoji 完全一致），仅：

- 标题用「昨日（DATE）」
- 空集兜底 alt 入口指 /find / /touched_yesterday（避免循环 → today /
  search_yesterday）

考虑过 inner helper 抽 common — 但既有 today_done / yesterday split /
digest split / touched split / search split 模板都是 clone — 跟随风格
+ 单测点稳定。两 fn diff < 6 行，维护成本可忽略。

### `src-tauri/src/telegram/bot.rs`

Handler 紧贴 SearchToday：

```rust
TgCommand::SearchYesterday { keyword } => {
    let views = read_tg_chat_task_views(chat_id.0);
    let yesterday = chrono::Local::now()
        .date_naive()
        .pred_opt()
        .unwrap_or_else(|| chrono::Local::now().date_naive());
    format_search_yesterday_reply(&views, yesterday, &keyword)
}
```

`pred_opt()` 与既有 /touched_yesterday / /digest_yesterday handler 同
pattern — chrono 自动处理跨月跨年；极端 NaiveDate::MIN 兜底走 today 防
panic。

## Key design decisions

- **clone 而非 generic helper**：与 today_done / yesterday split /
  digest split / touched_today/yesterday split / search_today split 既
  有模板一致 — 单测点稳定 + 行内逻辑 < 50 行
- **空集教学 loop prevention**：不指 /search_yesterday 自身（循环），
  也不指 /search_today（不同 scope 让 owner 困惑）— 指 /find（更广）+
  /touched_yesterday（同日期 broader scope），三阶 progressive disclosure
- **复用 SEARCH_TODAY_MAX_HITS 常量**：10 条 cap 与 /search_today / /find
  一致，owner 心智复用
- **4 个 unit tests pin 真实行为**（满足 GOAL.md "meaningful tests
  only"）：parser 1（含中文空格 keyword）+ 空 kw usage hint + 兜底 loop
  prevention（验证不含 /touched_today 这种 today 提示）+ today/yesterday
  date filter 交集 + status rank 不漏

## Verification

- `cargo build`（src-tauri）— clean
- `cargo test --lib` — all 1657 tests pass（新 4 + 既有 1653）
- 三个 drift-defense test all pass
- 手测：
  - 昨日 task 含 keyword → 「🔎 昨日（DATE）命中「kw」N 条」+ list
  - 昨日无命中 → 友好兜底 + alt 入口（/find / /touched_yesterday）
  - 今日 task 含 keyword → 不出现在 reply 里（date filter 验）

## Future iters (out of scope)

- `/search_in_detail_today <kw>` / `_yesterday` — 含 detail.md 内容搜
  + 日期 filter（与 /find_in_detail 同 axis 扩 today/yesterday）；后
  续 propose
- `/search_thisweek <kw>` — 周维度；按需评估
