# TG bot 加 `/digest_yesterday [N]` 命令（iter #522）

## Background

完整 yesterday × done × result-or-not 矩阵此前缺一格：

|               | 仅标题       | + result preview     |
|---------------|--------------|----------------------|
| 不限日期 done | /recent      | /digest              |
| 昨日 done     | /yesterday   | **/digest_yesterday** ← new |
| 昨日任意状态  | /touched_yesterday | (with HH:MM + done.result, no general result preview) |

owner 早上 standup 前 / 周五整理本周产出场景需要「昨天 done 列表 + 当时
怎么做」— /yesterday 仅标题不够，/digest 不限日期会混入今日。本 iter
补完矩阵。

## Changes

### `src-tauri/src/telegram/commands.rs`

按 6+ surface 模式同步：

1. **Enum 变体** `DigestYesterday { n: u32 }`（紧贴 MuteToday）
2. **`name()` arm** → `"digest_yesterday"`
3. **`title()` arm** → 加入 Digest 同 N-only arm 集
4. **parser arm**：与 /digest / /recent 同 N 处理（缺省 5，clamp 1..=20）
5. **en + zh registry** entries
6. **`ALL_HELP_TOPICS`** 加 `"digest_yesterday"`
7. **`format_help_for_topic("digest_yesterday")`** 详细文案（含 yesterday
   × done × result 矩阵 + 与 /digest / /yesterday / /touched_yesterday
   三方关系）
8. **`format_help_text`** 表格行
9. **两份 drift-defense test 列表**

#### 纯 formatter `format_digest_yesterday_reply`

```rust
pub fn format_digest_yesterday_reply(
    views: &[TaskView],
    yesterday: chrono::NaiveDate,
    n: u32,
) -> String {
    // filter Done + updated_at starts_with yesterday date prefix
    // sort by updated_at desc (latest first，与 /digest 同方向)
    // header: 📋 昨日（YYYY-MM-DD）完成 N 条（共 M）：
    // 每行 · HH:MM · title — result preview (80 char cap)
    // 不显 MM-DD（header 已含 date，避免冗余）
    // 空集教学指向 /digest / /yesterday / /touched_yesterday
}
```

与 `format_digest_reply` 差异点：

- 限 `updated_at.starts_with(yesterday_date)` 过滤
- header 用「昨日（DATE）」而非「最近 N 条」
- 行 ts 仅 HH:MM（date 已在 header）— 比 /digest 的 「MM-DD HH:MM」紧凑
- 空集教学 alt 入口三条（避免循环建议）

### `src-tauri/src/telegram/bot.rs`

Handler 紧贴 Digest 之前：

```rust
TgCommand::DigestYesterday { n } => {
    let views = read_tg_chat_task_views(chat_id.0);
    let yesterday = chrono::Local::now()
        .date_naive()
        .pred_opt()
        .unwrap_or_else(|| chrono::Local::now().date_naive());
    format_digest_yesterday_reply(&views, yesterday, n)
}
```

`pred_opt()` 与既有 /touched_yesterday handler 同 pattern — 跨月跨年
chrono 自动处理。

## Key design decisions

- **HH:MM only (no MM-DD)**：日期已在 header — 行内 MM-DD 重复冗余。与
  /touched_yesterday 同决策（也是 HH:MM only），与 /digest 的「MM-DD
  HH:MM」（不限日期需 MM-DD 区分）有意区分
- **同 80 char cap on result**：与 /digest / /yesterday / /touched_today
  / /touched_yesterday 同 result preview cap — owner 心智复用
- **三 alt 入口空集教学**：/digest（更广）/ /yesterday（仅标题）/
  /touched_yesterday（全谱）— 避免空集教学 dead end，不指向 own /
  digest_yesterday（防循环）
- **format clone 不抽 generic**：与 today_done / yesterday split / digest
  split 既有模板一致 — 单测点稳定 + 行内 < 50 行，维护成本可忽略
- **5 个 unit tests pin 真实行为**（满足 GOAL.md "meaningful tests
  only"）：parser 4 路径（default / explicit / clamp 21→20 / 非数字
  fallback）+ formatter 4 路径（空集教学 / done-only-yesterday 过滤 /
  HH:MM only without MM-DD / 80 char truncation）

## Verification

- `cargo build`（src-tauri）— clean
- `cargo test --lib` — all 1646 tests pass（新 5 + 既有 1641）
- 三个 drift-defense test all pass
- 手测：
  - 昨日有 done → 「📋 昨日（YYYY-MM-DD）完成 N 条」+ list with HH:MM
    + result preview
  - 昨日无 done → 友好兜底 + 三 alt 入口
  - clamp 21 → 20；abc → 5；纯数字 → N

## Future iters (out of scope)

- `/digest <date>` 任意日期版 — 通用化但 owner 主要需要 today /
  yesterday；按需 propose
- 「按 result preview length 过滤」chip — 找「写了长 result」的 done；
  audit 「正经做的 vs 走过场」
