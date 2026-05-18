# TG `/cat_growth_today` 命令（iter #599）— day × cat 矩阵第一档

## Background

cat × period audit 矩阵覆盖：
- day（今日切片）← 本 iter
- 7d（/cat_growth_7d / /cat_decay_7d）
- 30d（/cat_growth_30d / /cat_decay_30d）

补 day 档让 owner 「今天我在哪类知识投入」audit — 比 7d 信号更精细。

## Changes

6-surface 同步：

1. Enum `CatGrowthToday`（紧贴 `CatGrowth30d`）
2. name / parser / no-args title chain / registry en+zh / help-detail /
   help-table / 三份 drift lists
3. 新 pure `format_cat_growth_today_reply(rows, today: NaiveDate)`
4. Handler inline scan + `created_at.starts_with(today_str)` filter +
   sort desc。**不复用 compute_cat_growth_rows(threshold_days)** — today
   走 prefix match 与既有 today-family（/tags_today / /touched_today）
   一致，比 ts ≥ cutoff_ms 更直观
5. /help_table cat 家族 + family detail map 同步加入

## Output

```
🌱 今日（YYYY-MM-DD）各类新增（共 N 条 across M cats）：
· butler_tasks · +5
· decisions · +2
...
```

空 → 「今日各 cat 都没新建 item」+ 教学指 /cat_growth_7d 看更广 scope。

## Key design decisions

- **今日用 prefix match 而非 cutoff_ms**：`it.created_at.starts_with
  (today_str)` 比 `t.timestamp_millis() >= today_start_ms` 直观 — 与
  既有 /tags_today / /touched_today / /search_today 等 today-family
  pattern 一致
- **separate formatter 而非复用 format_cat_growth_reply**：今日 header
  「今日（DATE）」与 N 天「近 N 天」语义不同 — 单独 formatter 干净，
  也让兜底教学指向不同 sibling（7d / 7d vs decay/30d）
- **空兜底指 7d 而非 30d**：owner 今日 0 净增时下一步通常想看更宽
  scope；7d 比 30d 更近 owner 当下心智
- **3 unit tests**：parser + 空兜底（含 today_str） + multi-row（含
  label==empty 走 仅-key 分支 + label!=key 走括号分支）

## Verification

- `cargo build` clean
- `cargo test --lib` — 1805 pass（新 3 + 既有 1802）
- 三份 drift-defense + /help_table cat 家族 spot check 仍 pass

## Future iters (out of scope)

- **`/cat_growth_yesterday`**：补 day-family — 昨日切片。「昨天 vs
  今天」对比。按需 propose；day 档已有 today 一般够用
- **`/cat_growth_thisweek`**：本周 calendar boundary 切片（自周一起）
  — 与 /cat_growth_7d rolling window 不同。重叠多按需
- **PanelMemory 顶 chip「📊 today」**：桌面端 today 切片 chip — 用
  当前 PanelMemory 📊 audit chip 已含 7d 净增，今日单独 chip 可作
  feature drill-down
