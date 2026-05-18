# TG bot 加 `/find_speech_yesterday <kw>` 命令（iter #556）

## Background

`/find_speech` 全量扫，`/find_speech_today` 是今日切片 — 但「昨天 pet
跟我聊过 X 没」复盘视角缺。本 iter 补 yesterday 切片，完成 speech ×
date 三件套（today / yesterday / 全量），与 find_in_detail × date / search
× date / digest × date / touched × date 等已有矩阵对齐。

## Changes

按 6+ surface 同步：

1. Enum `FindSpeechYesterday { keyword: String }`（紧贴 FindSpeechToday）
2. name / title / parser / registry en+zh / help-detail / help-table /
   两份 drift lists

#### `format_find_speech_yesterday_reply`

clone of `format_find_speech_today_reply`：

- header「昨日（DATE）」让 scope 一眼可见
- 行 HH:MM only（date 已在 header — 与 today 一致）
- 空集兜底教学指 /find_speech（更广）+ /find_speech_today（最近 sibling
  scope）— 不指 self / 不指 /last_speech（不是 yesterday 域里的入口）

### Handler

紧贴 FindSpeechToday — 读 speech_history.log + 解析每行 RFC3339 ts →
本地 date 比较 yesterday（在扫 keyword 前过滤）。yesterday = `today -
chrono::Duration::days(1)` 让跨月 / 跨年 / 跨 DST 都安全。

## Key design decisions

- **ts filter before kw match**：date 比较先（cheap），keyword scan 后
  — 减无效 snippet 计算（沿用 today 切片设计）
- **行 HH:MM only**：与 /find_speech_today / /find_in_detail_today /
  _yesterday / /touched_today / /alarms_today 等 single-day-scope 命令
  一致（date 在 header）；只有 /find_speech 全量是 MM-DD HH:MM 因跨日
  scope
- **复用 extract_find_in_detail_snippet**：snippet 60 字算法跨 detail.md
  / speech.log 一致
- **8-cap 一致**：与 /find_speech / /find_speech_today 同
- **空集兜底**：/find_speech（更广 — 解决「昨日空但近期有」迷茫）+
  /find_speech_today（今日 sibling scope alt）— 防 self-loop。意识地
  不指 /last_speech 因为那是「最近一条」而非昨日域里的 audit 入口
- **4 unit tests**：parser（multi-token kw）+ 空 kw usage hint + no-hit
  yesterday-specific fallback（header 字串与教学链接验证） + hits 渲染
  （ts + snippet）

## Verification

- `cargo build` clean
- `cargo test --lib` — 1714 pass（新 4 + 既有 1710）
- 三个 drift-defense test all pass（registry + 两份 unknown drift list
  均已加 `find_speech_yesterday`）

## Future iters (out of scope)

- `/find_speech_thisweek` — 完成 speech × date 三件套之外的 thisweek 维度
  补全；speech 量与日同序量级 — 本周 scope 可能 hits 偏多，需要决定
  cap 是 8 → 12 还是仍 8。按需 propose
