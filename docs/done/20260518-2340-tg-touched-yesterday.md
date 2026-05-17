# TG bot 加 `/touched_yesterday` 命令（iter #512）

## Background

iter #510 加了 `/touched_today`（任意状态 + 今日 updated_at audit）。owner
反馈复盘场景常用 — 早会前回顾「昨天动过哪些」/ 周末看工作日 backlog 演
化等都需要。但 today-only 视角覆盖不全：

- `/yesterday` — 仅 done — 「昨日完成产出」
- `/touched_today` — 今日任意状态 — 「今日动作全谱」
- **缺口**：昨日任意状态 — 「昨日动作全谱」

本 iter 加 `/touched_yesterday` 补齐 today × yesterday × done-only ×
all-status 四象限矩阵。

## Changes

### `src-tauri/src/telegram/commands.rs`

按 6+ surface 模式同步：

1. **Enum 变体** `TouchedYesterday`（紧贴 TouchedToday）— 无参
2. **`name()` arm** → `"touched_yesterday"`
3. **`title()` arm** → 加入无参 arm 集
4. **parser arm** `"touched_yesterday" => Some(TgCommand::TouchedYesterday)`
5. **en + zh registry** entries
6. **`ALL_HELP_TOPICS`** 加 `"touched_yesterday"`
7. **`format_help_for_topic("touched_yesterday")`** 详细文案（含 today×
   yesterday × done-only × all-status 四象限说明）
8. **`format_help_text`** 表格行
9. **两份 drift-defense test 列表**

#### 纯 formatter `format_touched_yesterday_reply`

clone `format_touched_today_reply` 的结构（filter / sort / emoji / preview
完全一致），区别仅：

- 标题用「昨日」而非「今日」
- 空集兜底教学指向 `/touched_today` / `/yesterday` / `/tasks`（避免
  循环建议「昨日空 → 看昨日」）

考虑过抽 inner helper 复用，但 today_done vs yesterday 既有模板就是双
fn 各自单测点稳定 — 保持一致性 + 单测可读性 > DRY。两 fn 行内逻辑 < 50
行，clone 维护成本可忽略。

### `src-tauri/src/telegram/bot.rs`

Handler 紧贴 TouchedToday：

```rust
TgCommand::TouchedYesterday => {
    let views = read_tg_chat_task_views(chat_id.0);
    let yesterday = chrono::Local::now()
        .date_naive()
        .pred_opt()
        .unwrap_or_else(|| chrono::Local::now().date_naive());
    format_touched_yesterday_reply(&views, yesterday)
}
```

`pred_opt()` 跨月跨年 chrono 自动处理；极端兜底（如 NaiveDate::MIN 减
1）走 fallback today — 现实中 owner 用 pet 时 NaiveDate 是 2026-* 不会
触发，仅防御性编码。

## Key design decisions

- **clone 而非 generic helper**：今日 / 昨日的 fn 结构在既有 today_done /
  yesterday 已有 split 模板 — 跟随风格，单测点稳定。两 fn diff 仅 4 行
  （label + empty hint），维护成本可忽略
- **不同空集教学**：yesterday 空时教学指向 today_today / yesterday /
  tasks — 不指向 /today_done（避免「昨日空 → 看 today_done」语义错位）
- **`pred_opt()` 边界防御**：chrono::NaiveDate::MIN.pred_opt() 返
  None；现实不可能触发，fallback 走 today 让命令永远不 panic
- **4 个 unit tests pin 真实行为**（满足 GOAL.md "meaningful tests
  only"）：parser 1 + 空集教学（含 yesterday-specific 教学验证）+ 日
  期过滤（仅昨日 included、今日排除）+ emoji + snooze + result preview
  复用验证（实际是 sanity 检查 clone 没改坏行为）

## Verification

- `cargo build`（src-tauri）— clean
- `cargo test --lib` — all 1628 tests pass（新 4 + 既有 1624）
- 三个 drift-defense test all pass
- 手测：
  - `/touched_yesterday` → 「📅 昨日（YYYY-MM-DD）动过 N 条」+ 任意状
    态 task
  - 昨日 nothing touched → 友好兜底 + 三 alt 入口（/touched_today /
    /yesterday / /tasks）
  - pending + snooze → 💤；普通 pending → ⏳

## Future iters (out of scope)

- `/touched <date>` 任意日期 — 通用化但 owner 大概率只需要 today /
  yesterday（已覆盖 95% 场景）；后续按需评估
- `/touched_thisweek` / `/touched_lastweek` 周维度 — 复盘 sprint 周
  视角；后续 propose
