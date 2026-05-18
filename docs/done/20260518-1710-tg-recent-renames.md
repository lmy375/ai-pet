# TG `/recent_renames [N]` 命令（iter #580）— global rename audit

## Background

iter #574 加 `/aliases <title>` — 单 task rename chain reconstruction。
缺 cross-task 视角：「我最近改了几个 task 名」/「我哪段时间 rename 集
中」。本 iter 加 global N-recent rename audit。

完成 rename audit 双视角：
- /aliases <title>: 单 task 时间线（vertical: title's history）
- /recent_renames [N]: cross-task 最近 N 条（horizontal: recent events）

## Changes

6-surface 同步：

1. Enum `RecentRenames { n: u32 }`（紧贴 `StreakPin`）
2. name / parser (N clamp 1..=20，缺省 5) / title chain / registry en+zh
   / help-detail / help-table / 三份 drift lists
3. 新 pure formatter `format_recent_renames_reply` 接受 rows + total
4. Handler scan butler_history.log 反向 → 取 action=='rename' 行 →
   `extract_was_from_snippet` 取 old → `format_timeline_ts` 标 MM-DD
   HH:MM → reverse 到 newest first + truncate N

## Output format

```
🔁 近 N 条 rename（共 M 条 retention 内）：
· MM-DD HH:MM · 「old」→「new」
· MM-DD HH:MM · 「old」→「new」
...
```

空 → 「butler_history 内无 rename event」+ 教学指 /aliases / /timeline。

## Key design decisions

- **`extract_was_from_snippet` 复用 iter #568 helper**：snippet 80 字截
  断 fallback「old title 不可解」与既有 /aliases / /timeline rename
  行渲染一致
- **`format_timeline_ts` 复用 MM-DD HH:MM**：与 /timeline / /aliases
  ts 视觉一致让多命令并用时 ts 对得上
- **不 chat-scope**：butler_history 是 global log 无 chat origin —
  与 /streak_pin (iter #579) 同 tradeoff。owner 通常单 chat 用 pet 可
  接受
- **header 显「共 M 条 retention 内」**：让 owner 知 cap 是 retention
  限非命令限。N=5 显但 retention 内 30 条时让 owner 决定加大 N
- **`rows.truncate(n)` 而非 slice**：truncate 后 caller 仍可访问 rows
  长度作 cap 状态（实际本 iter 后未用，但 future safety）
- **5 unit tests**：parser default + parser clamp（upper / lower）+
  empty fallback + multi-row（old → new arrow）+ total > shown header

## Verification

- `cargo build` clean
- `cargo test --lib` — 1761 pass（新 5 + 既有 1756）
- 三份 drift-defense test all pass

## Future iters (out of scope)

- **`/recent_renames since:7d`**：时间窗 filter — 仅显近 7 天 rename。
  当前按 N 数 cap；按时间窗需 ts parsing 路径
- **PanelTasks chip-bar「🏷 N rename」chip**：桌面端同 audit — 当前
  TODO 已含此项作下一 iter 候选
- **rename 频率告警**：每月 rename 数 > 阈值（如 20+）红 chip 提示
  「重命名节奏过频」— audit 「task 命名标准是否不稳」决策。需统计基线
- **`/recent_renames <title_pattern>`**：title fuzzy 过滤版 — 「项目
  X 的 rename history」audit。但与 /aliases <title> 重叠，先观望
