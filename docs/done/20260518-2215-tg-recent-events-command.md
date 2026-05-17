# TG bot 加 `/recent_events <title> [N]` 命令（iter #505）

## Background

`/timeline <title>` 显单 task 的完整 butler_history 演化（前 30 条，按
chronological 旧→新）— 完整 audit 视角。但 owner 想「这条 task **最近**
发生了啥」TL;DR 时 /timeline 返回的内容偏多，要往下滚到底才看到最新事件。

本 iter 加 `/recent_events <title> [N]` — 共享 /timeline 底层算法
（compute_timeline_entries），但 formatter 取**末尾 N 条**（最近优先）。

## Convention

- `/recent_events <title>` → 显最近 **5** 条事件
- `/recent_events <title> 10` → 显最近 10 条
- `/recent_events 1` → 单 token 数字视作 title（task 索引）— 不剥 N
- `/recent_events 1 10` → 第 1 条 task 最近 10 条事件
- N clamp `1..=20`（与 /recent / /digest / /show_speech 同协议）
- 末 token 数字 >20 / =0 → 不剥（不在范围），整个 arg 当 title

## Changes

### `src-tauri/src/telegram/commands.rs`

按 6+ surface 模式全量同步：

1. **Enum 变体** `RecentEvents { title: String, n: u32 }`（紧贴 Snippets 之后）
2. **`name()` arm** → `"recent_events"`
3. **`title()` arm** → 复用 Show/Peek/Dup/Timeline 同 title list
4. **parser arm**：trailing-N 剥取算法（≥2 tokens + 末 token 是 1..=20
   数字时剥）
5. **en + zh registry** entries
6. **`ALL_HELP_TOPICS`** 加 `"recent_events"`
7. **`format_help_for_topic("recent_events")`** 详细文案（含与 /timeline
   差异 + N 语义 + 单 token 数字解析约定 + 示例）
8. **`format_help_text`** 表格行
9. **两份 drift-defense test 列表**

#### 纯 formatter `format_recent_events_reply`

```rust
pub fn format_recent_events_reply(
    title: &str,
    entries: &[TimelineEntry],   // chronological 旧→新
    total_events: usize,
    n: u32,
) -> String {
    if entries.is_empty() {
        return format!(
            "📜 「{}」最近事件\n\n（butler_history 内无该 task ...）",
            title,
        );
    }
    let show_count = entries.len().min(n as usize);
    let start = entries.len().saturating_sub(show_count);
    let recent_slice = &entries[start..];
    let mut out = format!(
        "📜 「{}」最近 {} 个事件（共 {}）：\n\n",
        title, recent_slice.len(), total_events,
    );
    for e in recent_slice {
        // emoji + ts_short + body / markers — 与 format_timeline_reply 同
        ...
    }
    out
}
```

与 /timeline 的差异：

| | /timeline | /recent_events |
|---|---|---|
| 取向 | chronological 起头 | 末尾 N |
| Cap | 30（固定） | 1..=20（参数化） |
| 用途 | 完整演化 audit | 最近 TL;DR |
| 底层 | compute_timeline_entries | 同 |

### `src-tauri/src/telegram/bot.rs`

Handler 紧贴 Timeline 之前 — 同 resolve 三层（数字 index → fuzzy → 错
误候选）+ 同 task_get_detail 路径 + 同 compute_timeline_entries dedup
逻辑，差异仅在 formatter 调 `format_recent_events_reply(...n)`。

## Key design decisions

- **共享 compute_timeline_entries 路径**：两命令 dedup「无 marker 变化的
  连续 update」算法完全一致 — owner 看 /timeline 后再 /recent_events
  不会发现「这条 event /timeline 显但 /recent_events 没」的诡异歧义
- **clamp 1..=20**：与 /recent / /digest / /show_speech / /alarms 等
  既有 N-param 命令同上限 — 用户心智复用
- **单 token 数字一律视作 title**：避免「/recent_events 5」歧义。owner
  想 N=5 又指定 task 走两 token 显式
- **末 token 数字 >20 → 不剥**：保「99」/「100」等数字 ID title 不被
  误剥成 N
- **末 token 数字 0 → 不剥**：N=0 无意义（与 cancel_all_error confirm
  token 等 0=clear 协议无关）
- **空 title → handler missing-arg**：parser 不抢话，把空判 deferred 给
  handler 与 /show / /timeline / /dup 一致
- **不复用 format_timeline_reply**：差异在 slice 方向（前 vs 后）+ cap
  含义（30 fixed vs N param）+ header（"时间线" vs "最近 N 个事件"）—
  强行复用会引 flag 参数 + header 模板，分开更清晰
- **7 个 unit tests pin 真实行为**（满足 GOAL.md "meaningful tests only"）：
  - parser 4 个：default N / trailing N / oversize >20 不剥 / 空 title
  - formatter 3 个：empty history 友好兜底 / 取末尾 N chronological /
    N 超 entries.len 时 clamp

## Verification

- `cargo build`（src-tauri）— clean
- `cargo test --lib` — all 1610 tests pass（新 7 + 既有 1603）
- 三个 drift-defense test all pass
- 手测：
  - `/recent_events 整理 Downloads` → 5 条最近事件 + ✏️ ts · markers
  - `/recent_events 整理 Downloads 3` → 3 条
  - `/recent_events 1` → 第 1 条 task 5 条
  - `/recent_events 99` → title "99"（不剥），handler 走 fuzzy 候选

## Future iters (out of scope)

- `/since <title> <date>` — 显某日期以来的事件（vs N 计数）— 时间窗维度
- /recent_events 加 owner-action filter（仅显 pinned/silent/snooze 等 owner
  打的 markers，跳过 LLM update）— 当前 dedup 已覆盖大部分场景
