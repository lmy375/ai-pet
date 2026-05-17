# TG bot `/promote_all_p7 confirm` 紧急 sprint 批量升优先级（iter #411）

## Background

owner 在突发 deadline / sprint 收尾时想「把所有挂着的活儿都拉到高
优让 pet 立即优先」当前只能逐条 `/pri <title> 7` 或 `/promote
<title>`。N 条 task 手敲 N 次太慢。

本 iter 加一次性批量入口 `/promote_all_p7`，与 `/cancel_all_error`
同 confirm-required pattern — 一次操作改 N 条任务 priority 必须
带 `confirm` token 防误触。

## Changes

### `src-tauri/src/telegram/commands.rs`

#### 1. `TgCommand::PromoteAllP7 { confirmed: bool }` 变体

紧贴 `CancelAllError` 同位置（confirm-required 破坏性批量族）。
name() / title() 同模式登记。

#### 2. 解析

```rust
"promote_all_p7" => {
    let confirmed = title.trim().eq_ignore_ascii_case("confirm");
    Some(TgCommand::PromoteAllP7 { confirmed })
}
```

case-insensitive `confirm` token；其他 trailing token（`yes` /
`ok` 等）算未确认（防误触）。

#### 3. `format_promote_all_p7_reply` pure 函数

```rust
pub fn format_promote_all_p7_reply(
    confirmed: bool,
    targets_before: u32,
    promoted_ok: u32,
    promoted_err: u32,
) -> String
```

四态：
- `!confirmed + targets=0` → 「暂无可升级」兜底，无 confirm scolding
- `!confirmed + targets>0` → usage hint 含 N 条预览 + 全命令示例
- `confirmed + ok+err=0` → idle 兜底（"暂无可升级 ✨"）
- `confirmed + ok>0` → 「已批量升 N 条」+ 失败 warning（如有）+
  follow-up hint（/tasks / /pri）

#### 4. Registry + ALL_HELP_TOPICS + help-for-topic + table line +
   两处 drift-defense

en：`"Sprint mode: batch +1 priority on all active tasks (clamp 7) — requires `confirm`"`
zh：`"紧急 sprint：批量给本聊天 active task priority +1（clamp 7）— 需带 `confirm`"`

#### 5. 8 单元测试

覆盖 parse（无参 / confirm 大小写 / 其他 trailing 不算 confirm）+
formatter 四态（zero targets / targets demand confirm / confirmed
idle / all ok / partial failure）。

### `src-tauri/src/telegram/bot.rs`

handler 紧贴 `CancelAllError` 之前（confirm-required 批量族集
中）：

```rust
TgCommand::PromoteAllP7 { confirmed } => {
    let views = read_tg_chat_task_views(chat_id.0);
    let candidates: Vec<(String, u8)> = views.iter()
        .filter(|v| matches!(v.status, Pending | Error))
        .filter(|v| v.priority < 7)  // 已 ≥ P7 跳过避免无意义写
        .map(|v| (v.title.clone(), v.priority))
        .collect();
    let total = candidates.len() as u32;
    if !confirmed {
        format_promote_all_p7_reply(false, total, 0, 0)
    } else {
        let mut ok = 0; let mut err = 0;
        for (title, old) in &candidates {
            let new_pri = (*old).saturating_add(1).min(7);
            match task_set_priority(title.clone(), new_pri) {
                Ok(()) => ok += 1,
                Err(_) => err += 1,
            }
        }
        format_promote_all_p7_reply(true, total, ok, err)
    }
}
```

设计要点：
- **chat-scope**：`read_tg_chat_task_views(chat_id.0)` 仅本 chat 派
  单 — 与 `/cancel_all_error` 一致语义
- **active-only filter**：done / cancelled 跳过；error 仍算（与
  /forks / /blocked 同语义 — error retry 时也要 priority 信号）
- **priority < 7 pre-filter**：避免对已 P7+ 的 task 调 task_set_priority
  写一次 + 写 yaml + 触发 butler_history event；纯优化
- **saturating_add + min(7)**：clamp 7 上限；P6 → P7；理论上 candidates
  filter 已保证 < 7 但二次防御
- **failure 不阻断**：与 /cancel_all_error 同模式 — 单条失败累计
  不中断 batch，最终 formatter 显警告

## Key design decisions

- **clamp 7 而非 9**：P7 是「紧急」语义边界（P8 / P9 是 reserved
  for hot-button scenarios）。批量升上限 P7 让 owner 还能手动用
  /promote / /pri 8/9 精调到极端；批量直接到 9 会让 priority 通胀
- **不引「降级」对偶 /demote_all**：批量降级 use case 弱（紧急 sprint
  结束 owner 通常重新审视每条 task 而非一刀切回 P3）；先实现高频
  方向，对偶可后续按需补
- **不做 idempotent 短路**：confirmed=true 时若 candidates 空仍调
  formatter 返「idle」reply — owner 仍然得到反馈"你的命令执行了，
  但没什么可改"。比静默更友好
- **复用 task_set_priority 同步 fn**：与 /promote / /demote / /pri
  单条入口同后端 — strip-before-write 保持其它 markers 不动
- **手测自检**：build pass + 8 单测覆盖纯路径；end-to-end 验证需
  TG 客户端测（确认 confirm 流程）— 与既有 /cancel_all_error 同
  pattern，行为可靠

## Verification

- `cargo test --lib telegram::commands::tests::promote_all_p7` — 8 / 8 通过
- `cargo test --lib`（全表）— 1439 / 1439 通过
- `npx tsc --noEmit`（frontend）— clean（无变更）
- `npx vite build`（frontend）— clean (1.23s)
- 一处 Chinese-ASCII-quote-in-Rust-string 编译失败已修（"低优先 dump"
  → 「低优先 dump」），与既往 iter 同 bug 同 fix
