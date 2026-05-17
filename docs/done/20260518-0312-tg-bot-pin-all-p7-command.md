# TG bot `/pin_all_p7 confirm` 批量 pin 高优清单（iter #447）

## Background

iter #411 加 `/promote_all_p7` — 批量升 priority 把 active task 拉到
P7+。iter #440 加 `/touch_all_p7` — 批量刷 P7+ updated_at 让挂着的高
优重新冒头。但「P7+ 批量族」第三个语义缺位：**钉**。

owner 在 sprint 收尾 / 周末整理时常想「我把高优清单全钉住，让屏幕
/ TG 端的「📌」filter 一眼看到这批是重点」。逐条 /pin <title> 在 P7+
有 5-10 条时累 — 同 family pattern 该有一键批量。

本 iter 加 `/pin_all_p7` — 与 /touch_all_p7 / /promote_all_p7 同
confirm-token 模板，filter `priority >= 7 + 未 [pinned]`（已 pinned
跳过避免无意义写）+ 逐条 `task_set_pinned(title, true)`。

## Changes

### `src-tauri/src/telegram/commands.rs`

#### 1. `TgCommand::PinAllP7 { confirmed: bool }` 变体

紧贴 `TouchAllP7`（P7+ 批量族）。

#### 2. 解析

```rust
"pin_all_p7" => {
    let confirmed = title.trim().eq_ignore_ascii_case("confirm");
    Some(TgCommand::PinAllP7 { confirmed })
}
```

与 /promote_all_p7 / /touch_all_p7 / /cancel_all_error 同 confirm-token
模板：仅 trailing `confirm`（case-insensitive）算确认。

#### 3. `format_pin_all_p7_reply` pure 函数

4 态状态机（与 touch_all_p7 / promote_all_p7 同模板）：

- `!confirmed + targets=0` → 「暂无可 pin 的 P7+ active task」兜底
- `!confirmed + targets>0` → usage hint 含 N 条预览 + 全命令示例
- `confirmed + 0 changes` → 「✨ 无可 pin 候选」idle 兜底（全已 pinned）
- `confirmed + ok>0` → 「📌 已批量 pin N 条」+ 失败 warning（如有）+
  follow-up hint（/pinned / /tasks）

### `src-tauri/src/telegram/bot.rs`

handler 紧贴 `TouchAllP7`：

```rust
TgCommand::PinAllP7 { confirmed } => {
    let views = read_tg_chat_task_views(chat_id.0);
    let candidates: Vec<String> = views.iter()
        .filter(|v| matches!(v.status, Pending | Error))
        .filter(|v| v.priority >= 7)
        .filter(|v| !v.pinned)                   // 已 pinned 跳过
        .map(|v| v.title.clone())
        .collect();
    let total = candidates.len() as u32;
    if !confirmed {
        format_pin_all_p7_reply(false, total, 0, 0)
    } else {
        let mut ok = 0; let mut err = 0;
        for title in &candidates {
            match task_set_pinned(title.clone(), true) {
                Ok(()) => ok += 1,
                Err(_) => err += 1,
            }
        }
        format_pin_all_p7_reply(true, total, ok, err)
    }
}
```

设计：
- **filter `!v.pinned`**：已 pinned 跳过 — 避免无意义写（与 /touch_all_p7
  filter `>= 7` 同"先 pre-filter 候选集"模式）。预览 / executed 数都对得上
- **复用 task_set_pinned**：既有 backend，stripping 旧 markers 再 append
  `[pinned]` — 即使理论上 race condition 走到已 pinned 的也 idempotent
- **failure per-step 累计不阻断**：与 /promote_all_p7 / /touch_all_p7 同
  模式

### Registry + ALL_HELP_TOPICS + help-for-topic + table line + 2x drift defense

- 双 lang registry 各加一条（紧贴 touch_all_p7）
- ALL_HELP_TOPICS 紧贴 "touch_all_p7"
- format_help_for_topic 加长详细文案（含与 /touch_all_p7 / /promote_all_p7
  三命令对比矩阵）；/touch_all_p7 详细文案末追加 /pin_all_p7 交叉引用
- format_help_text 全表加 `/pin_all_p7 confirm` 一行
- 两处 drift-defense 测试列表加 "pin_all_p7"

### 8 单元测试

parse（无参 / confirm 大小写 / 其它 trailing 不算 confirm）+ formatter
4 + 1 态（zero targets / targets demand confirm / all ok / partial failure
/ confirmed zero changes idle）。

## Key design decisions

- **filter `priority >= 7 + !pinned` 双条件**：双 filter 让候选集严格 =
  "本次会变化的条目"；预览 N 与执行 N 对得上。如果只 filter `>= 7`，
  含已 pinned 的会被算入预览但执行无变化 → 预览数字误导
- **不为 P5+ / P3+ 加变种**：与 /promote_all_p7 / /touch_all_p7 决策一
  致 — 单边界 P7+ 已 cover「sprint 收尾」语义；细化边界让命令矩阵爆炸。
  owner 想 P5+ 也钉可走单条 /pin
- **复用 task_set_pinned（含 strip 旧 markers）**：backend helper 已稳定
  - strip 旧 markers 再 append 让重复调用也 idempotent；这条对批量场景
  特别重要（防 race condition 写双 marker）
- **不推 decision_log 每条 TaskPin**：与 task_set_pinned 单调用同模式 —
  pin 是 owner 偏好（与 due / snooze 同），非状态转移。decision_log 应
  保 状态转移（done / cancel / error）audit 价值更高
- **不为 idempotent 短路 / dedup**：每次都执行 task_set_pinned；filter
  已剔除已 pinned 的，但即使 race 走到已 pinned 的也无副作用（backend
  自带 idempotent）
- **不动 priority / due / body**：本命令仅加 [pinned] marker — backend
  `task_set_pinned` 写法保其它 markers / due / body 不动

## Verification

- `cargo test --lib telegram::commands::tests::pin_all_p7` — 8 / 8 通过
- `cargo test --lib`（全表）— 1507 / 1507 通过（+8 from 1499）
- `npx tsc --noEmit`（frontend）— clean（无前端变更）
