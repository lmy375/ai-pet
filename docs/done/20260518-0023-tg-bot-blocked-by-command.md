# TG bot `/blocked_by <title>` 命令（iter #429）

## Background

依赖关系视图既有：
- `/blocked` — 全 chat audit 跨任务列被卡的 + 每条的 blockers
- `/forks <title>` — 反向：列被 `[blockedBy: <title>]` 引用的 task
  （「解锁 title 后谁会动起来」）

但**单条 task 的 blockers**没有专用入口 — owner 想「这条 task 在
等什么」必须 `/show <title>` 看 raw_description 含 `[blockedBy:
...]` markers + 自己心算 active vs done blocker。

本 iter 加 `/blocked_by <title>` — 单条 audit 列 title 仍未解决的
blockers（active 集合内的引用对象）。已 done / cancelled 的
blocker 视作已解决跳过。

## Changes

### `src-tauri/src/telegram/commands.rs`

#### 1. `TgCommand::BlockedBy { title }` 变体

紧贴 `Forks`，命名空间相邻（依赖关系视图族）。

#### 2. 解析

```rust
"blocked_by" => Some(TgCommand::BlockedBy { title }),
```

snake_case `blocked_by` 避开 dash drift-defense；与 /forks 同
single-title pattern。

#### 3. `format_blocked_by_reply(views, target)` pure 函数

5 态状态机：
- 空 target → usage hint
- target 在 views 找不到 → "没找到 task" 错误
- target.blocked_by 为空 → "无 markers — 不在等任何 blocker"
- 所有 blocker 已解决 → "✨ N 条 blocker 均已解决，可以推进了"
- 有未解决 → "🔒 被 N 条 blocker 卡住（共 M / N 仍未解决）" + 列表

```rust
pub fn format_blocked_by_reply(views, target_title) -> String {
    let target_view = views.iter().find(|v| v.title == target)?;
    let active: HashSet<&str> = views.iter()
        .filter(|v| matches!(v.status, Pending | Error))
        .map(|v| v.title.as_str())
        .collect();
    let unresolved: Vec<&str> = target_view.blocked_by.iter()
        .filter(|b| active.contains(b.trim()))
        .map(|s| s.as_str())
        .collect();
    // render with 🟢 (pending) / ⚠️ (error) icons
}
```

设计要点：
- **active 集合 = Pending | Error**：与 /blocked 同一致 — error
  retry 时仍是 blocker
- **trim 比较 blocker title**：与 /forks / `unresolved_blockers`
  算法一致；description 内 `[blockedBy: ...]` payload 周围空白
  容忍
- **icon 用 blocker 的 status**：blocker view 找到时按其 status
  渲（🟢 pending / ⚠️ error）— 让 owner 看「我等的是没人做 vs
  失败 retry 中」
- **header 显 total + unresolved 两个数**：让 owner 同时知道「我
  写过 N 条 blockedBy marker，其中 M 仍卡着」上下文

### `src-tauri/src/telegram/bot.rs`

handler 紧贴 `Forks`（3 层 title resolve 与 /forks 同模板）：

```rust
TgCommand::BlockedBy { title } => {
    if title.trim().is_empty() {
        format_missing_argument("blocked_by")
    } else {
        let actual = match try_resolve_by_index(...).await {
            Some(t) => Ok(t),
            None => resolve_tg_task_title(&title),
        };
        match actual {
            Ok(t) => {
                let views = read_tg_chat_task_views(chat_id.0);
                format_blocked_by_reply(&views, &t)
            }
            Err(msg) => format_command_error(&msg),
        }
    }
}
```

### Registry + ALL_HELP_TOPICS + help-for-topic + table line + 2x drift defense

- 双 lang registry 各加（紧贴 forks）
- ALL_HELP_TOPICS 紧贴 "forks"
- format_help_for_topic 长详细文案（含与 /forks / /blocked 对比矩阵）
- /forks help 末追加 /blocked_by 交叉引用
- format_help_text 全表加一行
- 两处 drift-defense 测试列表

### 8 单元测试

parse（含 title / 空 title）+ formatter 6 个场景（empty target /
target not found / no blockedBy markers / all resolved / mixed
active + done blockers / trim 比较）。

## Key design decisions

- **不显已 resolved blocker**：与 /blocked 一致 — owner 关心「还
  卡在等什么」非「曾经等过谁」；historical resolved blocker 是噪音。
  total count 体现「曾写过 N 条」上下文足够
- **不递归解依赖（transitive blockers）**：仅直接 1 层。若 A
  blocked_by B + B blocked_by C，本命令列 A 显 B（不显 C）。
  transitive blocker chain audit 是另一 feature（图遍历）— 单
  task 视图保持简单
- **与 /forks 同 read pipeline**：reuse read_tg_chat_task_views +
  pure formatter — handler 极简
- **8 测试覆盖**：parse + 5 个 formatter 状态 + 2 个 edge case（trim
  / 多状态混合）

## Verification

- `cargo test --lib telegram::commands::tests::blocked_by` — 8 / 8 通过
- `cargo test --lib`（全表）— 1472 / 1472 通过
- `npx tsc --noEmit`（frontend）— clean（无变更）
- `npx vite build`（frontend）— clean (1.26s)
