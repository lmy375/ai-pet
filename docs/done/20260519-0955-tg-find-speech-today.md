# TG bot 加 `/find_speech_today <kw>` 命令（iter #553）

## Background

`/find_speech` 扫 speech_history.log 全文找 keyword — pet utterance 全
量搜。但「今天 pet 提过 X 吗」audit 场景下，历史 utterance 干扰命中。

本 iter 加 today 切片 — 限当日触发的 utterance 内搜。

## Changes

按 6+ surface 同步：

1. Enum `FindSpeechToday { keyword: String }`（紧贴 FindInDetailYesterday）
2. name/title/parser/registry en+zh/help-detail/help-table/两份 drift lists

#### `format_find_speech_today_reply`

clone of `format_find_speech_reply`：

- header「今日（DATE）」让 scope 一眼可见
- 行 HH:MM only（date 已在 header — 与 /find_in_detail_today / /touched
  _today 一致）
- 空集兜底教学指 /find_speech / /last_speech（不指 self）

### Handler

紧贴 FindSpeech — 读 speech_history.log + 解析每行 RFC3339 ts → 本地
date 比较 today（在扫 keyword 前过滤，IO 已读全 log 但 string ops 少）。

## Key design decisions

- **ts filter before kw match**：date 比较先（cheap），keyword scan 后
  （contains 操作）— 减无效 snippet 计算
- **行 HH:MM only**：与 /find_in_detail_today / /touched_today /
  /alarms_today 等 today-scope 命令一致（date 在 header）；/find_speech
  全量是 MM-DD HH:MM 因跨日 scope
- **复用 extract_find_in_detail_snippet**：snippet 60 字算法跨 detail.md
  / speech.log 一致
- **8-cap 一致**：与 /find_speech / /find_in_detail 系列相同
- **空集兜底**：/find_speech（更广 — 解决「今日空但近期有」迷茫）+
  /last_speech（最近 1 条 fallback）— 避免 self-loop
- **4 unit tests**：parser（multi-token kw）+ 空 kw usage hint + no-hit
  today-specific fallback + hits 渲染（ts + snippet）

## Verification

- `cargo build` clean
- `cargo test --lib` — 1710 pass（新 4 + 既有 1706）
- 三个 drift-defense test all pass

## Future iters (out of scope)

- `/find_speech_yesterday` / `_thisweek` — 完成 speech × date 三件套；
  按需 propose
