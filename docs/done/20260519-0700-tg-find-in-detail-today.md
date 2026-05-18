# TG bot 加 `/find_in_detail_today <kw>` 命令（iter #543）

## Background

`/find_in_detail <kw>` 扫所有 task 的 detail.md 内容（IO 重，cap 8）。
但常用场景是「我今天在某主题写过什么笔记」 — 全量扫所有 task detail.md
IO 不划算 + 历史 detail 干扰当下 audit。

本 iter 加 `/find_in_detail_today` — 限今日 updated_at 的 task scope。

## Changes

### `src-tauri/src/telegram/commands.rs`

按 6+ surface 模式同步：

1. **Enum 变体** `FindInDetailToday { keyword: String }`（紧贴
   SearchThisweek 之后）
2. **`name()` arm** → `"find_in_detail_today"`
3. **`title()` arm** → 与 FindInDetail 同 keyword 列
4. **parser arm**：与 /find_in_detail 同 single-arg 模板
5. **en + zh registry** entries
6. **`ALL_HELP_TOPICS`** 加 `"find_in_detail_today"`
7. **`format_help_for_topic("find_in_detail_today")`** 详细文案
8. **`format_help_text`** 表格行
9. **两份 drift-defense test 列表**

#### 纯 formatter `format_find_in_detail_today_reply`

```rust
pub fn format_find_in_detail_today_reply(
    hits: &[FindInDetailHit],
    keyword: &str,
    today: chrono::NaiveDate,
) -> String {
    let kw = keyword.trim();
    if kw.is_empty() { return "🔬 用法 ..."; }
    let today_str = today.format("%Y-%m-%d").to_string();
    if hits.is_empty() {
        return format!("🔬 今日（{}）无 task 的 detail.md 含「{}」 ...");
    }
    // header「🔬 今日（DATE）命中「kw」N 条」+ 每行 emoji + title + snippet
    // 复用 8 hits cap + FindInDetailHit 既有 struct
    ...
}
```

复用既有 `FindInDetailHit` struct + `extract_find_in_detail_snippet` —
不引新 snippet 算法。

### `src-tauri/src/telegram/bot.rs`

Handler 紧贴 FindInDetail 之前：

```rust
TgCommand::FindInDetailToday { keyword } => {
    let kw = keyword.trim().to_string();
    let today = chrono::Local::now().date_naive();
    if kw.is_empty() {
        format_find_in_detail_today_reply(&[], &keyword, today)
    } else {
        let today_str = today.format("%Y-%m-%d").to_string();
        let views = read_tg_chat_task_views(chat_id.0);
        // 关键优化：filter today 在扫 detail.md 之前 — 大幅缩 IO scope
        let mut sorted: Vec<&TaskView> = views.iter()
            .filter(|v| v.updated_at.len() >= 10 && &v.updated_at[..10] == today_str.as_str())
            .collect();
        sorted.sort_by_key(|v| status_rank(&v.status));
        // ... 对 sorted 内每条 read detail.md + extract snippet
        format_find_in_detail_today_reply(&hits, &keyword, today)
    }
}
```

## Key design decisions

- **filter today BEFORE detail.md IO**：把 date 过滤放 read_to_string
  前，IO 仅扫今日 N 个 task（通常 N << 全量），比 /find_in_detail 的
  全量扫快很多 — 这是本命令相对 /find_in_detail 的最大优势
- **复用既有 `FindInDetailHit` + `extract_find_in_detail_snippet`**：
  60 字 context snippet 算法 / status emoji map / 8 cap 都和
  /find_in_detail 一致 — owner 切换 today vs 全量时输出格式无差异
- **header「今日（DATE）」**：与 /search_today / /tags_today / /alarms_today
  等 today-scope 命令一致 — owner 一眼看 scope
- **空集兜底教学指 /find_in_detail / /touched_today**：avoid loop；指
  broader scope（/find_in_detail）和同 scope no-kw（/touched_today）
- **clone formatter 不抽 generic**：与 search/touched/digest 各 today
  split 既有模板一致；行内 < 60 行
- **5 个 unit tests pin 真实行为**：parser（含 multi-token kw）+ 空 kw
  usage hint + no-hit fallback（含日期 + alt 入口） + 含 emoji + snippet
  渲染 + 8 cap + remainder hint

## Verification

- `cargo build`（src-tauri）— clean
- `cargo test --lib` — all 1684 tests pass（新 5 + 既有 1679）
- 三个 drift-defense test all pass
- 手测：
  - 今日 task 的 detail.md 含 "API" → 「🔬 今日（DATE）命中「API」N 条」+ snippets
  - 全无命中 → 友好兜底（/find_in_detail + /touched_today alt）
  - 昨日 task 含 "API" 的 detail.md → 不出现（today filter 验）

## Future iters (out of scope)

- `/find_in_detail_yesterday <kw>` / `/find_in_detail_thisweek <kw>` —
  完成 detail-content 搜索 × 时间 scope 三件套；按需 propose
- 「按 snippet length / 命中次数 sort」— 当前默认 status rank（pending
  浮顶）；按需
