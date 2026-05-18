# TG bot 加 `/search_thisweek <kw>` 命令（iter #539）

## Background

完成 fuzzy-search × date 三件套矩阵：

|         | today  | yesterday  | thisweek |
|---------|--------|------------|----------|
| no kw   | /touched_today | /touched_yesterday | /touched_thisweek |
| kw      | /search_today | /search_yesterday | **/search_thisweek** ← new |

owner 写周报 / 月度复盘需要「本周 + 主题」交叉筛选时，/find 太广，
/touched_thisweek 无 kw 太杂。本 iter 补缺口。

## Naming pivot

TODO 原写法 `/find_thisweek`，本 iter 改为 `/search_thisweek` 保命名
一致：

- search_* family（search_today / search_yesterday / search_thisweek）
  专用于「日期 scope + keyword」交集
- find_* family（find / find_in_detail / find_speech）专用于「不同
  source scope（task / detail.md / speech.log）」全量

混用两 verb 会让命令名预测困难。`/search_thisweek` 跟 search_* 系列。

## Changes

### `src-tauri/src/telegram/commands.rs`

按 6+ surface 模式同步：

1. **Enum 变体** `SearchThisweek { keyword: String }`（紧贴 SearchYesterday）
2. **`name()` arm** → `"search_thisweek"`
3. **`title()` arm** → 与 SearchToday / SearchYesterday 同 keyword 列
4. **parser arm**：与 /search_today / /search_yesterday 同 single-arg
   模板
5. **en + zh registry** entries
6. **`ALL_HELP_TOPICS`** 加 `"search_thisweek"`
7. **`format_help_for_topic("search_thisweek")`** 详细文案
8. **`format_help_text`** 表格行
9. **两份 drift-defense test 列表**

#### 纯 formatter `format_search_thisweek_reply`

```rust
pub fn format_search_thisweek_reply(
    views: &[TaskView],
    week_start: chrono::NaiveDate,  // 本周一日期
    keyword: &str,
) -> String {
    let kw = keyword.trim();
    if kw.is_empty() {
        return "🔎 用法：/search_thisweek <keyword>...".to_string();
    }
    let week_start_str = week_start.format("%Y-%m-%d").to_string();
    let kw_lower = kw.to_lowercase();
    let hits: Vec<_> = views.iter()
        .filter(|v| v.updated_at.len() >= 10 && &v.updated_at[..10] >= week_start_str.as_str())
        .filter(|v| v.title.to_lowercase().contains(&kw_lower) || v.raw_description.to_lowercase().contains(&kw_lower))
        // ... pending/error 浮顶 + 10 cap
}
```

与 `format_search_today_reply` 差异：

- 日期过滤改为 ≥ week_start prefix（不是 == today_str）
- header 显「本周（YYYY-MM-DD 起）」让 scope 一眼明确
- 空集兜底教学指 /find（全量）/ /touched_thisweek（本周全谱）— 避免
  loop 到 /search_today（不同 scope）或 /search_thisweek 自身

### `src-tauri/src/telegram/bot.rs`

Handler 紧贴 SearchYesterday：

```rust
TgCommand::SearchThisweek { keyword } => {
    use chrono::Datelike;
    let views = read_tg_chat_task_views(chat_id.0);
    let today = chrono::Local::now().date_naive();
    let days_from_mon = today.weekday().num_days_from_monday() as i64;
    let week_start = today - chrono::Duration::days(days_from_mon);
    format_search_thisweek_reply(&views, week_start, &keyword)
}
```

复用 `num_days_from_monday()` 与既有 `/touched_thisweek` handler 同
week_start 算法（Mon=0..Sun=6） — 周一起算。

## Key design decisions

- **clone 不抽 generic helper**：与既有 search/touched/digest 各
  today/yesterday/thisweek split 模板一致 — 单测点稳定 + 行内 < 60 行
- **prefix 字典序日期比较**：`updated_at[..10] >= week_start_str` —
  ISO 日期 prefix 10 字符；与 /touched_thisweek 同算法
- **空集兜底不指 /search_today / /search_thisweek 自身**：avoid loop +
  不同 scope alt 指向，指 /find（更广）/ /touched_thisweek（同 scope
  无 kw）让 owner 知道 broader / narrower 路径
- **复用 SEARCH_TODAY_MAX_HITS（10 cap）**：与 search 系列一致心智复用
- **rename pivot from TODO**：TODO 写 `/find_thisweek` 但 search_* 系
  列更一致 — 命令名应可预测；doc 记下 rename 原因
- **4 个 unit tests pin 真实行为**：parser（中文 + 多 token）+ 空 kw
  usage hint + 兜底 loop-prevention 验证（不指 /search_today）+ week
  + kw 双重过滤（this-week-hit included / this-week-miss excluded /
  last-week-hit excluded）

## Verification

- `cargo build`（src-tauri）— clean
- `cargo test --lib` — all 1674 tests pass（新 4 + 既有 1670）
- 三个 drift-defense test all pass
- 手测：
  - 本周有命中 → 「🔎 本周（DATE 起）命中「kw」N 条」+ status-sorted
    list
  - 本周无命中 → 友好兜底（/find / /touched_thisweek alt 入口）
  - 上周 task 含命中 → 不出现在 reply（date filter 验）

## Future iters (out of scope)

- `/search_lastweek <kw>` — 上周对偶；按需 propose
- `/search_thismonth <kw>` — 月维度；后续按需
- 状态过滤变种 `/search_pending_thisweek` 等 — 当前 sort 已浮顶 pending，
  专门子命令稀少需求
