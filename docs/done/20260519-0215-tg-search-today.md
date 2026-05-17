# TG bot 加 `/search_today <kw>` 命令（iter #523）

## Background

「今日 + keyword」交集场景此前需要两步：

1. `/touched_today` 列今日全谱
2. 肉眼扫找含 keyword 的条目

或：

1. `/find <kw>` 列所有命中（不限日期）
2. 肉眼挑「今天 updated」的

两条路径都 friction。本 iter 补缺口 — 今日 + kw 交集精准 audit。

三件套形成完整 fuzzy 搜索矩阵：

|                   | 不限日期       | 限今日                |
|-------------------|----------------|-----------------------|
| 无 kw             | /tasks        | /touched_today        |
| 含 kw             | /find         | **/search_today** ← new |

## Changes

### `src-tauri/src/telegram/commands.rs`

按 6+ surface 模式同步：

1. **Enum 变体** `SearchToday { keyword: String }`（紧贴 DigestYesterday）
2. **`name()` arm** → `"search_today"`
3. **`title()` arm** → 与 Find / FindInDetail / FindSpeech 同 single-arg
   keyword 列
4. **parser arm**：与 /find 同 single-arg 模板
5. **en + zh registry** entries
6. **`ALL_HELP_TOPICS`** 加 `"search_today"`
7. **`format_help_for_topic("search_today")`** 详细文案（含三件套定位
   矩阵 + 与 /find / /touched_today 互补关系）
8. **`format_help_text`** 表格行
9. **两份 drift-defense test 列表**

#### 纯 formatter `format_search_today_reply`

```rust
pub const SEARCH_TODAY_MAX_HITS: usize = 10;

pub fn format_search_today_reply(
    views: &[TaskView],
    today: chrono::NaiveDate,
    keyword: &str,
) -> String {
    // 空 kw → usage hint + alt 入口教学
    // filter: updated_at.starts_with(today) AND (title|raw_description).contains(kw_lower)
    // sort: pending → error → done → cancelled 浮顶（与 /find 同 rank）
    // header: "🔎 今日（YYYY-MM-DD）命中「<kw>」N 条："
    // 每行: <status emoji> <title>
    // 余量 hint cap 10
    // 空命中 → 友好兜底 + alt 入口 (/find / /touched_today)
}
```

与 `format_find_reply` 差异点：

- 额外 `updated_at.starts_with(today_date)` filter
- header 含「今日（DATE）」让 owner 一眼确认 scope
- 空命中兜底教学 alt 入口三条（避免「今日空 → 看今日」循环）
- 复用同 10 hits cap + status rank

### `src-tauri/src/telegram/bot.rs`

Handler 紧贴 Find 之前：

```rust
TgCommand::SearchToday { keyword } => {
    let views = read_tg_chat_task_views(chat_id.0);
    let today = chrono::Local::now().date_naive();
    format_search_today_reply(&views, today, &keyword)
}
```

## Key design decisions

- **复用 /find 状态 emoji + ranking**：心智一致 — owner 切换 /find ↔
  /search_today 无需学新视觉协议
- **空命中教学指 /find + /touched_today**：避开「今日空 → 看今日」循
  环；让 owner 知道两个 alt scope
- **header 含日期 + kw 双信息**：确认搜索 scope + keyword 都对，paste
  到 chat 给同事看也无歧义
- **不显 `[result:]` preview**：与 /find 同 — 本命令是「找位置」入口，
  详情走 /show；result preview 在 /digest_yesterday / /touched_today
  等 audit 入口
- **format clone 不抽 generic**：与 today_done / yesterday split / digest
  split / touched_today split / oldest_done split 既有 split 模板一
  致 — 单测点稳定 + 行内 < 50 行，维护成本可忽略
- **7 个 unit tests pin 真实行为**（满足 GOAL.md "meaningful tests
  only"）：parser 2（含空 kw）+ formatter 5（usage hint / no-hit
  friendly fallback / today-AND-keyword 双重过滤 / pending 浮顶 /
  case-insensitive）

## Verification

- `cargo build`（src-tauri）— clean
- `cargo test --lib` — all 1653 tests pass（新 7 + 既有 1646）
- 三个 drift-defense test all pass
- 手测：
  - 今日有命中 → 「🔎 今日（DATE）命中「kw」N 条」+ status-sorted list
  - 今日无命中 → 友好兜底 + 两 alt 入口
  - case-insensitive 验证（小写 kw 匹配大小写混合 title）

## Future iters (out of scope)

- `/search_yesterday <kw>` — 同模板昨日；按需 propose
- `/search_today_detail <kw>` — 含 detail.md 内容搜（与 /find_in_detail
  类比 + today filter）；后续 propose
- 状态过滤 chip：`/search_today_pending <kw>` 等 — 当前 sort 已浮顶
  pending，专门子命令稀少需求
