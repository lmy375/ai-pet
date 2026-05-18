# memory_rename 加 butler_history record_event 调用（iter #568）

## Background

iter #567 /aliases pivot drop 揭示：memory_rename 不写 butler_history —
所有历史 audit 命令（/timeline / /recent_events / 未来 /aliases）都看
不到 rename 事件。本 iter 修这个 substrate gap。

## Changes

### Backend：memory_rename 加 record_event 调用

`commands/memory.rs:memory_rename` 在 `write_index` + mirror 成功后，
对 `category == "butler_tasks"` 推 rename event 到 butler_history.log：

```rust
if category == "butler_tasks" {
    let old_for_log = old_title.clone();
    let new_for_log = new_trimmed.clone();
    tauri::async_runtime::spawn(async move {
        crate::butler_history::record_event(
            "rename",
            &new_for_log,
            &format!("[was: {}]", old_for_log),
        )
        .await;
    });
}
```

- **fire-and-forget**：record_event 是 async 但 memory_rename 是 sync；
  spawn 让 IO 在背景跑不阻塞主 path 返回。best-effort 与既有
  `butler_task_edit` tool 的 record_event 调用语义一致
- **`category == "butler_tasks"` gate**：butler_history.log 是
  butler_tasks 专属 log；其它 cat（todo / task_archive / general 等）
  rename 不污染。/timeline / /recent_events 仅扫 butler_tasks（与
  filter_history_for_task 一致）
- **`[was: <old>]` 标记 protocol**：snippet 内嵌 prefix；formatter 端
  parse 取 old title 复原。format 由本 iter 与 formatter wiring 共同
  约定

### Formatter wiring：解 [was: X] + 渲新行

`TimelineEntry` struct 加 `was: Option<String>` 字段（derive Default
让既有 init 可用 `..Default::default()`）。`compute_timeline_entries`
在 action_lc == "rename" 时调 `extract_was_from_snippet(snippet)` 把
old title 拎进 entry。

`extract_was_from_snippet`（新 pure helper）：
- 找首个 `[was: ` prefix → 截到首个 `]`
- 若无 `]`（80 字截断切走尾巴 / 异常）→ 取到 snippet 末尾，剥末尾 `…`
- 空 / 仅空白 → None

`format_timeline_reply` + `format_recent_events_reply` 加 rename 分支：
- emoji: 🔁（与 /cascade_rename emoji 同）
- body: `重命名 from 「<old>」`（或 was=None 时 fallback「重命名
  （old title 不可解）」— 截断时仍可见是 rename 而非误判为「无 marker
  变化」update）

### Test fixtures 修正

`TimelineEntry { ... }` init 8 处加 `was: None,`（4 个测试函数内）。
TimelineEntry 加 Default derive 让未来扩字段时 fixtures 升级更轻。

新 4 unit tests：
- `extract_was_from_snippet_basic`：基本 case + noise prefix + 截断 +
  无 prefix + 空 value 5 个变种
- `timeline_reply_renders_rename_with_old_title`：rename + Some(old)
  → 「🔁 重命名 from 「old」」
- `timeline_reply_renders_rename_with_unknown_old_fallback`：rename +
  None → 「🔁 重命名（old title 不可解）」（仍可识别）
- `recent_events_reply_renders_rename`：同 timeline 但走 recent_events
  formatter，确认两路径都生效

## Key design decisions

- **`[was: ...]` 而非 markers 协议**：考虑过把 `was` 作为新 marker
  key 走 extract_marker_tokens — 但 `was` 不是状态变化 marker，是
  metadata（与 `[task pri=...]` / `[origin:...]` 同类）。formatter 端
  分支处理更清晰，markers 关键字白名单不被污染
- **80 字截断 fallback**：long old title 可能被切但 `[was: ` prefix
  6 字 + 部分 old → 仍能 parse 出 partial old title。比直接 None 体
  验好；用 `trim_end_matches('…')` 处理 record_event 端可能加的省略号
- **不对 update / create / delete 触发 was 解析**：仅 action_lc ==
  "rename" 时 extract。避免误识别正常 update 里巧合的 `[was: ` 文本
- **action 大小写不敏感**：to_ascii_lowercase() 比 record_event 写入
  的小写 "rename" 严格化，未来若手工写 log 带 "Rename" 也能识别
- **memory_rename gate to butler_tasks**：其它 cat 的 rename（如
  manual `memory_rename(general, ...)`）不写 butler_history。若未来需
  /memory_aliases 跨 cat audit，可加 generic memory_history.log

## Verification

- `cargo build` clean
- `cargo test --lib` — 1731 pass（新 4 + 既有 1727）
- 既有 8 个 TimelineEntry fixture 全部升级 was: None；无 test 回归

## Future iters (unblocked by 本 lift)

- **TG `/aliases <title>`**：扫 butler_history.log 找 rename event 重建
  alias chain — title 在 rename 行可作 new (head 显) 或 old（snippet
  内）。双向 walk 让 owner 看「这条曾叫过：A → B → C」
- **`/timeline <title>` 含 rename 渲**：本 iter 已完成 — `/timeline`
  调出来就含 🔁 行
- **rename history 旧 record 兼容**：本 lift 起的 rename 才被 log；
  pre-lift rename 永远不可见。文档 future iter 可在 README 加段说明
  pre-本 iter 数据局限
- **PanelTasks rename → desktop notification**：rename 是 owner action，
  桌面端 notify「task X renamed to Y」也许有用，按需 propose
