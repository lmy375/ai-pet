# TG `/audit_summary` 命令（iter #587）— sprint kickoff 聚合视图

## Background

近月加了 30+ audit 命令（pin / cat / rename / idle / streak / find /
speech / 等矩阵）。owner 周一早会想 5 秒看「上周怎样 / 本周从哪起」
需翻 5-10 个命令：/streak / /streak_pin / /idle_7d / /touched_today /
/pinned / /pin_grow_7d / /cat_growth_7d。低效。

本 iter 加单命令 `/audit_summary` 聚合 5 大 audit 信号一行式，每条
后跟 deep dive 入口让 owner 想细看时一步直达。

## Output

```
📋 audit summary（YYYY-MM-DD）
· 📌 pin streak: N 天连续（当前 M 钉）→ /streak_pin
· 🌱 cat 7d 净增: K cat 活跃 → /cat_growth_7d
· 💤 idle 7d+: P 条 stale pending → /idle_7d
· ✅ 今日 touched: Q 条 → /touched_today
· 🏷 近 7d rename: R 次 → /recent_renames
```

## Changes

6-surface 同步：

1. Enum `AuditSummary`（紧贴 `HelpTable`）
2. name / parser / no-args title chain / registry en+zh / help-detail /
   help-table / 三份 drift lists
3. 新 pure `format_audit_summary_reply(today, 5 signals)` formatter
4. Handler 聚合 5 signal：
   - `compute_pin_streak` (复用 iter #579 helper)
   - `compute_cat_growth_rows(7)` (复用 iter #575 helper)
   - chat-scoped views filter for idle 7d / touched_today
   - butler_history scan for recent_renames_7d
   - **单次 butler_history 扫**同时收 dates_with_pin + recent_renames_7d，
     避免双 IO
5. **更新 `/help_table`**：system 家族列表 + family detail map 都加
   `/audit_summary`

## Key design decisions

- **单 butler_history 扫**：handler 内 dates_with_pin 与 renames count
  共享一次 read_history_content() — IO 仅 1 次而非 2 次。每行 check
  starts_with("rename ") + contains("[pinned]") 两路径并行
- **5 信号选择**：pin / cat / idle / today touched / rename — 覆盖
  attention（pin）+ growth（cat）+ stale audit（idle）+ activity（touched）
  + meta（rename）。done streak 留给 /streak 单命令（不重复 — owner
  可串「/audit_summary && /streak」）
- **deep dive 入口每行尾**：避免「audit_summary 只数字不引导」复述
  痛点。owner 想细看哪行直接 /xxx 跳
- **`AuditSummary` no-arg**：在 no-args title chain。0 args invariant
- **零值仍渲染**：避免「这行没数据是不是 bug」歧义。 0 streak / 0
  idle 等都明示
- **3 unit tests**：parser + formatter all signals + zero values

## Verification

- `cargo build` clean
- `cargo test --lib` — 1776 pass（新 3 + 既有 1773）
- 三份 drift-defense test all pass
- /help_table 测试 still pass（add 后未变 assertion 路径）

## Future iters (out of scope)

- **更多 signal slot**：/audit_summary 可扩展到 7-10 signal — done
  streak / consolidate freshness / 等。但行数太多失「sprint kickoff」
  紧凑性。先观望
- **`/audit_summary detail`** 长版：每 signal 给一行更详细描述 + 多
  deep dive。当前紧凑模式优先
- **桌面端 PanelDebug「📋 audit」chip**：单一 chip click 弹 modal 显
  /audit_summary 等价内容。需 backend Tauri 命令封装聚合
- **`/audit_summary <date>`**：历史 snapshot — 看过去某天的 audit 状
  态。需历史快照 / 重放 — substrate 缺
