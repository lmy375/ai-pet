# TG bot `/touch <title>` 命令（iter #435）

## Background

owner 在 TG 端看到一条挂了很久的 active task 想让 pet 重新主动
关注 — 当前没有"轻触"入口：要么 /promote 改 priority（语义太
强 — 不该升优先级，只想让它"冒头"）、要么 /pri 重设（同问题）、
要么 /edit 改 description（重写过头）。

本 iter 加 `/touch <title>` — 刷 updated_at 不改内容。机制与既
有 task_skip_once 同（rewrite 同 description → memory_edit 自动
stamp updated_at），但 decision_log 标 `TaskTouch` audit 区分意
图：skip = 跳本轮 fire；touch = 让老 task 重新冒头。

## Changes

### `src-tauri/src/commands/task.rs`

#### 1. 抽 `rewrite_description_to_bump_updated` helper

把 task_skip_once_inner 的核心逻辑（rewrite same description →
memory_edit auto-stamps updated_at）抽到私有 helper，inject decision
label 参数。

```rust
fn rewrite_description_to_bump_updated(
    title: String,
    decisions: DecisionLogStore,
    decision_label: &str,
) -> Result<(), String> {
    // existing skip_once logic + 用 decision_label 替代硬编 "TaskSkipOnce"
}
```

#### 2. `task_skip_once_inner` 改为 trampolined caller

```rust
pub fn task_skip_once_inner(title, decisions) -> Result<(), String> {
    rewrite_description_to_bump_updated(title, decisions, "TaskSkipOnce")
}
```

#### 3. 新 `task_touch_inner` helper

```rust
pub fn task_touch_inner(title, decisions) -> Result<(), String> {
    rewrite_description_to_bump_updated(title, decisions, "TaskTouch")
}
```

错误信息也用 `decision_label.to_lowercase()` 动态拼："cannot
touch a finished task" / "cannot taskskip_once a..." — 让 user-
facing error 与命令名一致。

### `src-tauri/src/telegram/commands.rs`

#### 1. `TgCommand::Touch { title }` 变体

紧贴 `TagsFor`（single-title 命令族）。

#### 2. 解析

```rust
"touch" => Some(TgCommand::Touch { title }),
```

与 /show 同 single-title pattern。

#### 3. `format_touch_reply(title, save_ok)` pure 函数

```rust
match save_ok {
    Ok(()) => "✨ 已 touch「<title>」— updated_at 已刷新，老任务重新冒头 proactive 选单。",
    Err(e) => "✨ touch 失败：<msg>",
}
```

空 title → usage hint 含机制说明 + 与 /skip 关系 + done/cancelled
拒绝注。

#### 4. Registry + ALL_HELP_TOPICS + help-for-topic + table + drift defense

完整 6 处（en + zh registry / ALL_HELP_TOPICS / format_help_for_topic
/ format_help_text / 两处 drift-defense 测试列表）。

### `src-tauri/src/telegram/bot.rs`

handler 紧贴 `TagsFor`：

```rust
TgCommand::Touch { title } => {
    if title.trim().is_empty() {
        format_touch_reply(&title, Ok(()))
    } else {
        let actual = three_layer_resolve(&title)...;
        match actual {
            Ok(t) => {
                let decisions = state.app.state::<DecisionLogStore>().inner().clone();
                let save = task_touch_inner(t.clone(), decisions);
                format_touch_reply(&t, save.as_ref().map(|_| ()).map_err(|e| e.as_str()))
            }
            Err(msg) => format_command_error(&msg),
        }
    }
}
```

reuse 既有 3 层 title resolve + decision_log store pattern。

### 5 单元测试

parse（含 title / 空 title）+ formatter 3 个状态（empty usage /
success / failure error）。

## Key design decisions

- **共享 backend helper 而非 copy-paste**：把 task_skip_once_inner
  抽 rewrite_description_to_bump_updated helper — 两 caller 注入
  不同 decision label。避免逻辑漂移
- **decision_log label 区分意图**：mechanism 相同但 audit log 应
  反映 owner 实际命令 — /skip 时 log 标 TaskSkipOnce / /touch 时
  标 TaskTouch，让历史回溯准确
- **done / cancelled 拒**：终态 task touch 后 updated_at 改了但
  status 仍 done — 不会回到 proactive 选单，无意义；拒绝防 owner
  困惑「我 touch 了为什么没动」
- **与 /skip 共享同 backend**：rewrite same description → updated_at
  stamps 是 chain reaction；两 command 实际 mutation 完全一致
- **5 测试覆盖**：parse 2 个 + formatter 3 个状态（empty usage /
  success / error）

## Verification

- `cargo test --lib telegram::commands::tests::touch` — 5 / 5 通过
- `cargo test --lib`（全表）— 1483 / 1483 通过
- `npx tsc --noEmit`（frontend）— clean（无变更）
- `npx vite build`（frontend）— clean (1.26s)
