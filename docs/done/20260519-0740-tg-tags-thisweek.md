# TG bot 加 `/tags_thisweek` 命令（iter #545）

## Background

完成 tags × date 三件套矩阵：

|         | 不限日期 | 今日           | 昨日                | 本周                |
|---------|----------|----------------|---------------------|---------------------|
| #tag 聚合 | /tags  | /tags_today   | /tags_yesterday     | **/tags_thisweek** ← new |

周报场景下 owner 写「本周我在哪些主题工作」总览 — 本 iter 补缺口。

## Changes

按 6+ surface 同步：

1. **Enum 变体** `TagsThisweek`（紧贴 TagsYesterday）
2. `name()` arm → `"tags_thisweek"`
3. `title()` arm → 无参 arm 集
4. parser arm
5. en + zh registry entries
6. ALL_HELP_TOPICS / help-detail / help-table / 两份 drift-defense lists

#### `format_tags_thisweek_reply`

clone `format_tags_yesterday_reply`：

- 改日期 filter 为 `>= week_start_str`（ISO 字典序，与
  /touched_thisweek / /search_thisweek 同算法）
- header「本周（YYYY-MM-DD 起）N 个 tag」
- 空集兜底教学指 /tags / /tags_today / /touched_thisweek（avoid loop）

### Handler

紧贴 TagsYesterday — `chrono::Datelike::num_days_from_monday()` 算
days_from_mon → today - days = week_start，与 /touched_thisweek /
/search_thisweek 同 week_start 算法。

## Key design decisions

- **clone 不抽 generic**：与既有 today/yesterday/thisweek split 模板
  一致 — 单测点稳定 + 行内 < 50 行
- **`>= week_start_str` prefix 字典序比较**：与既有
  /touched_thisweek / /search_thisweek 同协议
- **空集兜底 alt 入口**：/tags（更广）/ /tags_today（同 axis today）/
  /touched_thisweek（同 date 全谱）— avoid self-loop
- **3 个 unit tests**：parser + 空集（含 week-specific date + alt 入口
  教学）+ 周内 vs 上周分隔（this-week-tag-in / last-week-tag-out）

## Verification

- `cargo build` clean
- `cargo test --lib` — 1690 pass（新 3 + 既有 1687）
- 三个 drift-defense test all pass
- 手测：
  - 本周有 #tag task → 「🏷 本周（DATE 起）N 个 tag」+ count list
  - 本周全无 #tag → 友好兜底
  - 上周 #tag 不计入

## Future iters (out of scope)

- `/tags_lastweek` — 上周对偶；按需 propose
- `/tags <date>` 任意日期 — 通用化按需
