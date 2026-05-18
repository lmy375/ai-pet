# TG bot 加 `/find_in_detail_yesterday <kw>` 命令（iter #546）

## Background

Detail-content × date 矩阵进展：

|                  | 不限日期      | 今日                  | 昨日 ← new            |
|------------------|---------------|-----------------------|-----------------------|
| detail.md 搜索 | /find_in_detail | /find_in_detail_today | **/find_in_detail_yesterday** |

复盘场景：早会前回忆「昨天我在 detail.md 写过 X 相关的进度」/ 写日报
需「昨天在某主题的笔记」audit。

## Changes

按 6+ surface 同步：

1. Enum 变体 `FindInDetailYesterday { keyword: String }`（紧贴
   FindInDetailToday）
2. name / title / parser / registry en+zh / help-detail / help-table /
   两份 drift-defense lists

#### `format_find_in_detail_yesterday_reply`

clone of today version：

- date filter 改 yesterday
- header「昨日（DATE）」
- 空集兜底教学指 /find_in_detail（更广）+ /touched_yesterday（同日期
  全谱），不指 today（不同 scope，避免 owner 困惑）/ 不指 self

### Handler

紧贴 FindInDetailToday — chrono::pred_opt() 算昨日，**filter yesterday
在扫 detail.md 之前** 让 IO 限定到小集合。与 today 版同优化。

## Key design decisions

- **filter before detail.md IO**：与 today 版同 — 限 date 后 IO scope
  小，比 /find_in_detail 全量扫快
- **复用 FindInDetailHit + extract_find_in_detail_snippet**：snippet
  算法 60 字 context 不分裂；status emoji map 一致；8-cap 同 /find_in_detail
  系列
- **clone 不抽 generic**：与既有 today/yesterday/thisweek split 模板
  一致
- **空集兜底 alt 入口避免 today loop**：不指 /find_in_detail_today（不
  同 date scope）— 让 owner 知道 broader scope (/find_in_detail) 或同
  scope full-spec (/touched_yesterday)
- **4 unit tests**：parser（含 multi-token kw）+ 空 kw usage hint +
  no-hit yesterday-specific fallback（含 loop-prevention 验证 — 不指
  /find_in_detail_today）+ 含 emoji + snippet 渲染

## Verification

- `cargo build` clean
- `cargo test --lib` — 1694 pass（新 4 + 既有 1690）
- 三个 drift-defense test all pass
- 手测：
  - 昨日 task 的 detail.md 含 "API" → 「🔬 昨日（DATE）命中「API」N 条」
    + snippets
  - 全无命中 → 友好兜底
  - 今日 task 含命中的 detail.md → 不出现（yesterday filter 验）

## Future iters (out of scope)

- `/find_in_detail_thisweek <kw>` — 本周 detail-content scope；按需 propose
- /find_speech_today/_yesterday — speech_history.log 的 date scope；
  按需
