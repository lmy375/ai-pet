# TG bot 加 `/alarms_thisweek` 命令（iter #547）

## Background

Alarms × date 矩阵进展：

|         | 不限日期 N    | 今日           | 本周                  |
|---------|---------------|----------------|-----------------------|
| reminder | /alarms [N] | /alarms_today  | **/alarms_thisweek** ← new |

周报场景 / 周一 review 上周未消 alarm 需本周 scope 视图。

## Changes

按 6+ surface 同步：

1. Enum `AlarmsThisweek`（紧贴 AlarmsToday）+ name/title/parser/registry
   /help-detail/help-table/两份 drift lists

#### `format_alarms_thisweek_reply`

clone `format_alarms_today_reply`：

- filter 改为 target.date() 落在 `week_start..=week_end_inclusive`
  （week_end = week_start + 6 天）
- TodayHour 按定义算本周 included
- 跨日 scope → 行 MM-DD HH:MM（与 /alarms 同；alarms_today 单日 scope
  仅 HH:MM）
- 空集兜底教学指 /alarms / /alarms_today / /touched_thisweek

### Handler

紧贴 AlarmsToday — 同 read path（todos_as_memory_items +
parse_reminder_prefix）+ 同 sort（target asc）；week_start 与
/touched_thisweek / /search_thisweek / /tags_thisweek 同
`num_days_from_monday()` 算法。

## Key design decisions

- **inclusive week range**：`d >= week_start && d <= week_end` 让周日
  晚 23:59 触发的 alarm 仍算本周。week_end 不溢出（chrono Date 7-天
  加法安全）
- **TodayHour 永远本周**：按定义 today 落在本周（除非跨周日午夜，但
  TodayHour 命名上就是 today）
- **跨日 scope → MM-DD HH:MM**：与 /alarms 全量同；与 /alarms_today
  单日 HH:MM 有意区分
- **复用 format_target_short**：与 /alarms 行渲染同 helper（自适应
  date vs HH:MM）
- **clone 不抽 generic**：与既有今日/本周 split 模板一致
- **3 unit tests**：parser + 空集兜底（含日期 + alt 入口）+ 包含性范围
  过滤（Mon/Sun/TodayHour in；上周日/下周一 out）

## Verification

- `cargo build` clean
- `cargo test --lib` — 1697 pass（新 3 + 既有 1694）
- 三个 drift-defense test all pass
- 手测：
  - 本周有 alarm → 「⏰ 本周（DATE 起）N 条 alarms」+ MM-DD HH:MM 行
  - 本周无 alarm → 友好兜底
  - 上周/下周 alarm 不计入

## Future iters (out of scope)

- `/alarms_lastweek` — 历史 fire audit；按需 propose
- `/alarms <date>` 任意日期 — 通用化按需
