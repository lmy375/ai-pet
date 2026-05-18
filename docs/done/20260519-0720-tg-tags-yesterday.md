# TG bot 加 `/tags_yesterday` 命令（iter #544）

## Background

iter #542 加 `/tags_today` 后，tags × date 矩阵今日填上。昨日缺口：
owner 写日报 / 复盘「昨天我在哪些主题工作」时需要昨日 tag 聚合。

矩阵进展：

|         | 不限日期      | 今日           | 昨日                    | 本周                |
|---------|---------------|----------------|-------------------------|---------------------|
| #tag 聚合 | /tags        | /tags_today    | **/tags_yesterday** ← new | /tags_thisweek (todo) |

## Changes

按 6+ surface 同步：

1. **Enum 变体** `TagsYesterday`（紧贴 TagsToday）
2. `name()` arm → `"tags_yesterday"`
3. `title()` arm → 无参 arm 集
4. parser arm
5. en + zh registry entries
6. ALL_HELP_TOPICS / help-detail / help-table / 两份 drift-defense lists

#### `format_tags_yesterday_reply`

clone `format_tags_today_reply`：

- 改日期 filter（yesterday_str）
- header 用「昨日（DATE）N 个 tag」
- 空集兜底教学指 /tags（全量）/ /tags_today / /touched_yesterday —
  避免 loop（不指 self）

### Handler

紧贴 TagsToday — chrono::pred_opt() 算昨日日期，与既有 /touched_yesterday
/ /digest_yesterday / /search_yesterday 同模板。

## Key design decisions

- **clone 不抽 generic**：与 today/yesterday split 系列既有模板一致
  （each fn < 50 lines，独立单测点）
- **空集兜底 alt 入口避免循环**：不指 /tags_yesterday 自身，也不指
  /search_yesterday（不同 axis）— 指 /tags（更广）/ /tags_today（同
  axis today）/ /touched_yesterday（同 date 全谱）
- **3 个 unit tests**：parser + 空集（验 alt 入口避循环）+ 日期过滤
  （yesterday-tag-in / today-tag-out）

## Verification

- `cargo build` clean
- `cargo test --lib` — 1687 pass（新 3 + 既有 1684）
- 三个 drift-defense test all pass
- 手测：
  - 昨日 task 含 #tag → 「🏷 昨日（DATE）N 个 tag」+ count list
  - 昨日全无 #tag → 友好兜底
  - 今日 task #tag 不计入

## Future iters (out of scope)

- `/tags_thisweek` — 本周 scope（TODO 内还有一项）
- `/tags <date>` 任意日期 — 通用化按需 propose
