# TG bot `/timeline <title>` 命令（iter #400）

## Background

`/show <title>` 给 owner 看任务**当前 snapshot**（raw_description 含全部
markers + detail.md 预览），但答不了「这条 task 经历了啥」— 比如 owner
回看「写周报」时想知道：什么时候 pin 的、什么时候 snooze 推迟过、什么
时候补 `[result:]` 的、有没有 retry 失败的痕迹。

butler_history.log 已记录所有 create/update/delete 事件含
description snippet — 信号就在那但当前没单条 task 的 audit 入口
（PanelTasks 任务详情面板有但需开桌面）。本 iter 加 TG `/timeline
<title>` 命令把 butler_history 按时序展开 + 扫每个事件的「状态变化」
markers — 让 owner 一句话从手机 audit 单条 task 的历史。

## Changes

### `src-tauri/src/telegram/commands.rs`

#### 1. `TgCommand::Timeline { title }` 变体

与 `Show` 同 single-title pattern。空 title 由 handler 走 missing-arg。

#### 2. `extract_marker_tokens(snippet) -> Vec<String>` pure helper

扫 snippet 内 `[<key>...]` 段，白名单 key：

- `done` / `error` / `snooze` / `result` / `cancelled` / `pinned` /
  `silent` / `blockedBy` / `archived`

白名单外的 `[task pri=...]` / `[origin:...]` / `[every:...]` /
`[once:...]` / `[deadline:...]` / `[remind:...]` / `[tags:...]` 等静态
元数据**不**入 timeline — 它们是任务身份非状态变化信号。

key 后允许 ` ` / `:` / `：` / `]` 收口防止 `[doneish]` / `[errorlike:]`
等前缀碰撞误命中。无闭合 `]` 时优雅跳出不 panic。

返回的是**完整原文** `[done]` / `[result: 已发送]` — 保 payload 让 owner
看到 result / error / snooze 等具体内容。

#### 3. `compute_timeline_entries(events_newest_first)` pure helper

`filter_history_for_task` 返回 newest-first；本 fn reverse 到 chronological
（旧→新）+ 扫 marker tokens + **去重无变化 update**：

- 第一个事件总保
- `action != "update"`（即 create / delete）总保
- update 且 markers 集合（含 payload 文本）变化 → 保

这把「LLM 多次 update detail.md 但 markers 不动」的噪声 update 去掉，
保留状态变化转折点。payload 变化（`[snooze: A]` → `[snooze: B]`）算变
化，因比较的是 marker token 全文。

#### 4. `format_timeline_ts(rfc3339)` pure helper

抽 `YYYY-MM-DDTHH:MM:SS+08:00` 形式的 `MM-DD HH:MM` 短显示。string
slicing 而非 chrono 重解析 — robust 且零依赖。形式不识别时兜底返完
整 ts 不丢信息。

#### 5. `TimelineEntry` 公开 struct + `format_timeline_reply` 文案

输出格式：

```
🕰️ 「<title>」时间线 · N 个事件（去重无变化 update 后保留 M 条）

📝 05-15 09:30 · 创建
✏️ 05-16 14:20 · [pinned]
✏️ 05-17 09:15 · [pinned] [snooze: 2026-05-20 18:00]
✏️ 05-17 21:00 · [pinned] [done] [result: 已发送给团队 lead]
```

设计要点：
- emoji：📝 create / ✏️ update / 🗑️ delete
- body：create→「创建」/ delete→「删除」/ 无 markers update→「更新
  （无 marker 变化）」/ 有 markers update→ markers 空格连接
- entries 空 → 兜底「butler_history 内无该 task 的事件记录…」并
  提示 `/show` 看 snapshot
- 30 条 cap + overflow hint（防 TG 4096 字符炸；典型 task 几条到十
  几条事件，30 足够）
- header 行 dedup count 仅在 `entries.len() < total_events` 时浮，
  让 owner 知道有被去重不被「我以为有 50 条 history 但只显 3 条」
  误会

### `src-tauri/src/telegram/bot.rs`

handler 模板与 `Show` 同三层 resolve（数字 index → fuzzy → 错误候选）：

```rust
TgCommand::Timeline { title } => {
    if title.trim().is_empty() {
        format_missing_argument("timeline")
    } else {
        let actual = match try_resolve_by_index(...).await {
            Some(t) => Ok(t),
            None => resolve_tg_task_title(&title),
        };
        match actual {
            Ok(t) => match task_get_detail(t).await {
                Ok(detail) => {
                    let raw_events = detail.history.iter()...collect();
                    let entries = compute_timeline_entries(&raw_events);
                    format_timeline_reply(&detail.title, &entries, raw_events.len())
                }
                Err(e) => format_command_error(&e),
            },
            Err(msg) => format_command_error(&msg),
        }
    }
}
```

复用既有 `task_get_detail` 后端（也是 `/show` 路径用的）—  no new IO
surface。

### Registry & help & drift defense

- `tg_command_registry_localized` 两 lang 加 `("timeline", "...")`
  条目（en + zh）
- `ALL_HELP_TOPICS` 列表加 `"timeline"`
- `format_help_for_topic` 加 `"timeline" => "🕰️ /timeline <title>..."`
  长详细文案
- `format_help_text` 全表加 `/timeline <title>  —  ...` 一行
- 两处 drift-defense 测试列表（`tg_command_registry_covers_all_user_facing_commands`
  + `format_help_for_each_listed_command_returns_detail`）加 `"timeline"`

## Key design decisions

- **复用 butler_history 而非 description 内 markers 自带 ts**：description
  里的 `[done]` `[result:]` 等 markers 不含时间戳 — 「何时打的标记」
  信号在 butler_history.log 的 event ts 上。本命令就是把这两个数据源
  接上 — 不用扩 description schema 加 ts payload。
- **白名单 markers 而非 blacklist**：static 元数据（task pri / origin /
  every / remind 等）会污染 timeline 信号，白名单更稳定 — 未来新 marker
  加进来 owner 也明确决定是否计入状态变化。
- **dedup unchanged update 但 force_keep create/delete**：连续 update
  无 marker 变化是 noise（detail.md silent edit / LLM 用 task_edit_tool
  改 body 不动 markers）；create/delete 是事实事件本身 owner 关心。
- **不显 detail.md 内容**：与 `/show` 分工 — show 看 snapshot 含 detail
  预览，timeline 看历史含 markers。两命令互补不重叠。
- **不引 frontend 镜像**：PanelTasks 任务详情面板已经有「历史」段（
  task_get_detail 同源），desktop owner 已有视图。本 iter 仅补 TG 端
  audit 缺口。
- **30 条 cap**：典型 task 几条到十几条；30 已是 95th percentile。超
  长 task（每日 every: 自动跑一年）会有上百条 update，但其中绝大多数
  会被 dedup 掉（marker 不变）— 实际能 surface 到 30 条以上是极端情
  况，overflow hint 让 owner 知道有截断。

## Verification

- `cargo test --lib telegram::commands::tests::timeline` — 19 / 19 通过
- `cargo test --lib`（全表回归）— 1405 / 1405 通过
- `npx tsc --noEmit`（frontend）— clean（无变更）
- `npx vite build`（frontend）— clean
