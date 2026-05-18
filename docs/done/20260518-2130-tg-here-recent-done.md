# TG `/here_recent_done` 命令（iter #595）— done-axis priming

## Background

完成 here-* 注入 family 5 命令：
- /here_pin: task 维度（在乎 / pinned）
- /here_idle: task 维度（搁着 / stale 7d+）
- /here_top_cat: cat 维度（主力研究范畴）
- /here_recent_done: task 维度（已完成 / done） ← 本 iter
- /here_clear: 撤回

owner 写周报 / 月度复盘 / 新建 follow-up task 前 prime pet 知最近成
就，让 reply 含 momentum 文案 / 自动 link 上下游。

## Changes

6-surface 同步：

1. Enum `HereRecentDone`（紧贴 `HereTopCat`）
2. name / parser / no-args title chain / registry en+zh / help-detail /
   help-table / 三份 drift lists
3. 新 pure `format_here_recent_done_reply(rows, until_local)`
4. Handler chat-scoped views filter done + sort by updated_at desc +
   take 5 → 拼「✅ 最近完成 context：「t1」「t2」...」 → set_transient
   _note 60
5. /help_table status 家族（done audit 归属）+ family detail map 同步

## Output

```
✅ 已注入 N 条 done task 到 transient_note（到 HH:MM 失效）
· 「整理 Downloads」（05-17 完成）
· 「写周报」（05-16 完成）
...
```

空 → 「无 done task — 完成一条再来」+ 教学指 /today_done / /digest。

## Key design decisions

- **5 条 hard-coded cap**：与 /digest / /recent 同 default cap — 紧凑
  context 不让 transient_note text 膨胀
- **`updated_at` 5..10 取 `MM-DD` ASCII safe**：RFC3339 前 10 字
  `YYYY-MM-DD` 中 5..10 = `MM-DD`；不依赖 chrono parse
- **inline text 仅 title（不含 date）**：注入 pet 的 prompt 仅 title
  list — pet 端 token 经济；ack reply 仍含 date 让 owner 看到何时完成
- **RFC3339 字典序 = chron 序**：用 `b.updated_at.cmp(&a.updated_at)`
  排 desc 安全 — ISO 8601 ASCII prefix 单调
- **3 unit tests**：parser + 空兜底 + multi-row（含 date label）

## Verification

- `cargo build` clean
- `cargo test --lib` — 1798 pass（新 3 + 既有 1795）
- 三份 drift-defense test all pass

## Future iters (out of scope)

- **`/here_recent_done [N]`**：参数 N — 当前 hard-coded 5。按需 propose
- **「last week's done」时间窗版**：`/here_done_week` — 周报场景。但
  与 /digest_thisweek + /here_recent_done 重叠
- **含 [result:] marker 注入**：每 done 行带 result snippet — pet 能
  引用具体成果 / 决策。需 marker 解析（与既有 /digest 一致）
