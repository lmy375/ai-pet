# TG bot 加 `/digest_thisweek [N]` 命令（iter #552）

## Background

完成 digest × date 三件套矩阵：

|         | 不限日期      | 昨日                | 本周                |
|---------|---------------|---------------------|---------------------|
| done + result | /digest | /digest_yesterday   | **/digest_thisweek** ← new |

写周报场景：owner 周五整理「本周完成 + result」时需要 done + result
preview 的周维度视图。

## Changes

按 6+ surface 同步：

1. Enum `DigestThisweek { n: u32 }`（紧贴 DigestYesterday）
2. name/title/parser/registry en+zh/help-detail/help-table/两份 drift lists

#### `format_digest_thisweek_reply`

clone of `format_digest_yesterday_reply`：

- date filter 改 `updated_at[..10] >= week_start_str`（ISO 字典序）
- header「本周（YYYY-MM-DD 起）完成 N 条（共 M）」
- 行 MM-DD HH:MM（跨日 scope；与 /digest 同；/digest_yesterday 是
  HH:MM only 因 single-day）
- 80 char result preview cap 一致

### Handler

紧贴 DigestYesterday — `num_days_from_monday()` 算 week_start，与
/touched_thisweek / /search_thisweek / /tags_thisweek / /alarms_thisweek
同算法。

## Key design decisions

- **MM-DD HH:MM per line**：与 /digest 同，跨日不能省 date；本命令与
  /digest_yesterday HH:MM-only（single-day）形成 daily vs week scope
  对偶
- **clone 不抽 generic**：与既有 today/yesterday/thisweek + digest /
  touched / search / tags / alarms 各 split 一致
- **空集兜底教学**：指 /digest（更广）/ /touched_thisweek（同 scope
  全谱）/ /yesterday（昨日 done）— 避免 self-loop + 不指 /digest_today
  （不存在；今日 done 走 /today_done）
- **复用 num_days_from_monday() week_start**：与所有 thisweek 命令同
  Monday-based 算法
- **4 unit tests**：parser（default / explicit / clamp / 非数字）+ 空集
  兜底（含日期 + 三 alt 入口）+ 周内 done filter（本周 done in / 本周
  pending out / 上周 done out）+ 跨日 MM-DD HH:MM 格式 + result preview

## Verification

- `cargo build` clean
- `cargo test --lib` — 1706 pass（新 4 + 既有 1702）
- 三个 drift-defense test all pass
- 手测：
  - 本周有 done → 「📋 本周（DATE 起）完成 N 条」+ MM-DD HH:MM list +
    result preview
  - 本周无 done → 友好兜底
  - 上周 done / 本周 pending → 不计入

## Future iters (out of scope)

- `/digest_lastweek` — 上周复盘；按需 propose
- `/digest_thismonth` — 月维度；后续按需
