# TG bot `/forks <title>` 命令（iter #403）

## Background

既有 `/blocked` 给 owner 看「我哪些 task 在等什么」— 以**被卡**为
视角列 active task + 它们仍未解决的 blockers。但反向问题没入口：
「我做完这条 blocker 会让谁动起来」— owner 决定优先级 / 紧迫感
时需要这个反向视图（"做完这条会松开几条 → 价值高" vs "做完这条
没有 fork → 可缓"）。

本 iter 加 `/forks <title>` 反向 audit：扫 chat-scoped views 找所
有 active task 的 `blocked_by` 含 target_title 的，列出来。

## Changes

### `src-tauri/src/telegram/commands.rs`

#### 1. `TgCommand::Forks { title: String }` 变体

与 `Show` / `Timeline` 同 single-title pattern。空 title 由 handler
走 missing-arg。

#### 2. `format_forks_reply(views, target_title) -> String` pure 函数

- 空 target → defensive usage hint（caller 用 missing-arg 兜底但
  避免直接调 fn 时 panic）
- 扫 views 筛 status ∈ {Pending, Error} + `blocked_by` 含 target
  trim 后字面相等的
- 无命中 → "🔱 解锁「<title>」不会影响其它 active task（叶子节点 /
  无引用方）"
- 有命中 → "🔱 解锁「<title>」会松开 N 条 task：" + 每行 🟢/⚠️ icon
  + dependent title

设计决策：
- **trim 比较 blocked_by 元素**：与 `unresolved_blockers` 算法
  一致 — description 内 `[blockedBy: ...]` payload 周围空白容忍
- **active-only filter**：done / cancelled 的 dependent 不算"会
  被松开"— 它们已超出 active 池
- **error 状态 dependent 算**：error retry 时同样需要 blocker
  解锁，与 /blocked 含 error 同语义
- **不去重 dependent title**：理论上一条 task 不会有两个相同的
  `[blockedBy:]` payload；如有 owner 自己 audit 容易 — 不复杂化
- **self-loop 不特判**：若 target 把自己列进 blocked_by（病态
  数据），formatter 不静默隐藏；测试 pin 了当前行为

### `src-tauri/src/telegram/bot.rs`

handler 模板与 `Show` / `Timeline` 同三层 title resolve：

```rust
TgCommand::Forks { title } => {
    if title.trim().is_empty() {
        format_missing_argument("forks")
    } else {
        let actual = match try_resolve_by_index(...).await {
            Some(t) => Ok(t),
            None => resolve_tg_task_title(&title),
        };
        match actual {
            Ok(t) => {
                let views = read_tg_chat_task_views(chat_id.0);
                format_forks_reply(&views, &t)
            }
            Err(msg) => format_command_error(&msg),
        }
    }
}
```

### Registry & help & drift defense

- `tg_command_registry_localized` 两 lang 加 `("forks", "...")`
  条目（en + zh）
- `ALL_HELP_TOPICS` 列表加 `"forks"`
- `format_help_for_topic` 加 `"forks" => "🔱 /forks <title>..."` 长
  详细文案（含输出格式样例 + 与 /blocked 对比）
- `format_help_text` 全表加 `/forks <title>  —  ...` 一行
- 两处 drift-defense 测试列表加 `"forks"`
- /blocked 帮助文案末加「相关：/forks」交叉引用

## Key design decisions

- **scope 同 chat**：与 /blocked / /tasks 一致用 `read_tg_chat_task_views`
  filter 当前 chat 的派单；跨 chat 的 dependent 不会显（与既有命令
  一致避免泄露不相干 chat 的 task 名）
- **不要 cap 输出条数**：典型一条 blocker 不会被 >10 task 引用；
  极端情况下 owner 看到长列表也是有价值的（说明这条解锁影响巨
  大）— 不像 /find 是 corpus 搜索需要 cap
- **不显 blocker 的 due / pri / 其它 meta**：fork 视图聚焦"谁会
  动"而非"谁动后会怎样"— meta 走 /show <title> 单独审计
- **复用 task_queue::TaskView**：blocked_by 字段已由
  build_task_view 解析好，formatter 仅做集合操作

## Verification

- `cargo test --lib telegram::commands::tests::forks` — 9 / 9 通过
- `cargo test --lib`（全表回归）— 1414 / 1414 通过
- `npx tsc --noEmit`（frontend）— clean（无变更）
- `npx vite build`（frontend）— clean
