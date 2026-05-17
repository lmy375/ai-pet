# TG bot `/touch_all_p7 confirm` 批量 touch 高优清单（iter #440）

## Background

iter #411 加 /promote_all_p7 — 把 active task priority +1 到 P7
上限。但「已是 P7+ 但挂着没动的高优」owner 想批量唤醒（让 pet 重
新关注）当前要逐条 /touch。

本 iter 加 `/touch_all_p7` — 与 /promote_all_p7 对偶但语义反向：
那个升 priority 让低优变高优；本命令仅刷已 P7+ 的 updated_at，
让"挂着的高优"重新冒头 proactive 选单。

## Changes

### `src-tauri/src/telegram/commands.rs`

#### 1. `TgCommand::TouchAllP7 { confirmed: bool }` 变体

紧贴 `PromoteAllP7`（confirm-required 批量族）。snake_case
`touch_all_p7` 避开 dash drift-defense。

#### 2. 解析

```rust
"touch_all_p7" => {
    let confirmed = title.trim().eq_ignore_ascii_case("confirm");
    Some(TgCommand::TouchAllP7 { confirmed })
}
```

与 /promote_all_p7 / /cancel_all_error 同 confirm-token 模板：仅
trailing `confirm`（case-insensitive）算确认，其它 trailing 视
为未确认（防误触）。

#### 3. `format_touch_all_p7_reply` pure 函数

4 态状态机（与 format_promote_all_p7_reply 同模板）：
- `!confirmed + targets=0` → 「暂无 P7+ active task」兜底
- `!confirmed + targets>0` → usage hint 含 N 条预览 + 全命令示例
- `confirmed + 0 changes` → 「✨ 无可 touch 候选」idle 兜底
- `confirmed + ok>0` → 「已批量 touch N 条」+ 失败 warning（如有）+
  follow-up hint（/tasks / /oldest_n）

### `src-tauri/src/telegram/bot.rs`

handler 紧贴 `PromoteAllP7`：

```rust
TgCommand::TouchAllP7 { confirmed } => {
    let views = read_tg_chat_task_views(chat_id.0);
    let candidates: Vec<String> = views.iter()
        .filter(|v| matches!(v.status, Pending | Error))
        .filter(|v| v.priority >= 7)   // 反向：只挑 P7+
        .map(|v| v.title.clone())
        .collect();
    let total = candidates.len() as u32;
    if !confirmed {
        format_touch_all_p7_reply(false, total, 0, 0)
    } else {
        let decisions = state.app.state::<DecisionLogStore>().inner().clone();
        let mut ok = 0; let mut err = 0;
        for title in &candidates {
            match task_touch_inner(title.clone(), decisions.clone()) {
                Ok(()) => ok += 1,
                Err(_) => err += 1,
            }
        }
        format_touch_all_p7_reply(true, total, ok, err)
    }
}
```

设计：
- **filter `priority >= 7`**：与 /promote_all_p7 的 `< 7` 反向 —
  那个升非 P7+ 到 P7；本命令仅 touch 已 P7+。两命令一起把"高优
  集合"整个唤醒
- **复用 task_touch_inner**：iter #435 已建 backend helper，
  decision_log 标 `TaskTouch`（每条 touch 都入 audit log）
- **failure per-step 累计不阻断**：与 /promote_all_p7 同模式

### Registry + ALL_HELP_TOPICS + help-for-topic + table line + 2x drift defense

- 双 lang registry 各加一条（紧贴 promote_all_p7）
- ALL_HELP_TOPICS 紧贴 "promote_all_p7"
- format_help_for_topic 加长详细文案（含与 /promote_all_p7 对比矩阵）
- /promote_all_p7 help 末追加 /touch_all_p7 交叉引用
- format_help_text 全表加 `/touch_all_p7 confirm` 一行
- 两处 drift-defense 测试列表加 "touch_all_p7"

### 7 单元测试

parse（无参 / confirm 大小写 / 其它 trailing 不算 confirm）+
formatter 4 态（zero targets / targets demand confirm / all ok /
partial failure）。

## Key design decisions

- **filter `priority >= 7` 而非 `> 7`**：P7 本身就是「高优」语义
  边界 — 包含 P7 是常识；/promote_all_p7 clamp 7 也包含 P7
- **不为 P5+ / P3+ 加变种**：单边界 P7+ 已 cover「高优唤醒」语义；
  细化边界让命令矩阵爆炸。owner 想 P5+ 也唤醒可走 /touch 单条
- **不动 description 内容**：与 task_touch_inner backend 行为一致
  — rewrite same description triggers updated_at bump but content
  identical；不污染原内容
- **decision_log 每条 TaskTouch**：批量也 per-item 入 log，让历
  史回溯能看到「这条 P7 在某时刻被批量唤醒」
- **不为 idempotent 短路 / dedup**：每次都执行 — 与 /promote_all_p7
  同；让 owner 重复点能反复刷 updated_at 把 task 推到队尾再队尾

## Verification

- `cargo test --lib telegram::commands::tests::touch_all_p7` — 7 / 7 通过
- `cargo test --lib`（全表）— 1490 / 1490 通过
- `npx tsc --noEmit`（frontend）— clean（无变更）
- `npx vite build`（frontend）— clean (1.36s)
