# TG bot `/pinned_due` 命令（iter #413）

## Background

owner 在 TG 端做"紧急清单 audit"时既有信号点散：
- `/pinned` 列所有 pinned（无 due 维度 — 含日历无截止的"长期重要"）
- `/due [preset]` 列时段内 due（无 pinned 维度 — 含没标记的杂项）
- `/markers` 列 pinned + silent 联合（无 due 维度）

"我钉了 + 有截止时间"两个高优信号交集 — 紧急冲刺 / deadline 收
尾时 owner 优先关注的清单 — 没有单条命令直接显。本 iter 加 `/pinned_due`
双重 filter 视图。

## Changes

### `src-tauri/src/telegram/commands.rs`

#### 1. `TgCommand::PinnedDue` 变体（无参）

紧贴 `Pinned`。snake_case 命名 `pinned_due` 避开 dash drift-defense。

#### 2. 解析

```rust
"pinned_due" => Some(TgCommand::PinnedDue),
```

无参 + 多余尾部忽略（与 /pinned / /silenced 同容忍）。

#### 3. `format_pinned_due_reply(views)` pure 函数

```rust
pub fn format_pinned_due_reply(views: &[TaskView]) -> String {
    let mut filtered: Vec<&TaskView> = views.iter()
        .filter(|v| matches!(v.status, Pending | Error))
        .filter(|v| v.pinned)
        .filter(|v| v.due.is_some())
        .collect();
    if filtered.is_empty() {
        return "🔥 暂无同时 pinned + 含 due 的 active task...";
    }
    filtered.sort_by(|a, b| {
        a.due.as_deref().unwrap_or("").cmp(b.due.as_deref().unwrap_or(""))
    });
    // header "🔥 pinned + due 任务（共 N 条，按 due 升序）" + 每行 format_task_line
}
```

设计：
- **三层 filter 串联**：active（done / cancelled 跳）+ pinned +
  has-due。filter 顺序无副作用但语义上"active 是粗筛"放最前
- **due ISO 字典序 = 时间序**：task_queue 标准化为
  `YYYY-MM-DDTHH:MM`，ASCII 字典序与时间序一致 — 直接 cmp 不调
  parse 防错
- **复用 format_task_line**：与 /pinned / /tasks 行格式一致；自动
  含 P<priority> + 截至 MM/DD HH:MM 摘要
- **truncate_if_overflow 收尾**：与 /pinned 同 TG 长度防御

### `src-tauri/src/telegram/bot.rs`

handler 紧贴 `Pinned`：

```rust
TgCommand::PinnedDue => {
    let views = read_tg_chat_task_views(chat_id.0);
    format_pinned_due_reply(&views)
}
```

formatter 内部做所有 filter — handler 只过 chat-scope，让单测稳定
（formatter 可直接 inject 任意 views 验证）。

### Registry + ALL_HELP_TOPICS + help-for-topic + table line + 2x drift-defense

- 双 lang registry 各加一条
- ALL_HELP_TOPICS 紧贴 "pinned"
- format_help_for_topic 加 `"pinned_due" => "🔥 /pinned_due..."` 长详细文案
- /pinned help 文案末追加交叉引用 /pinned_due
- format_help_text 全表加 `/pinned_due` 一行
- 两处 drift-defense 测试列表加 "pinned_due"

### 6 单元测试覆盖

parse（无参 + case-insensitive + 容忍尾随）+ formatter 5 个场景：
empty fallback / four-axis filter matrix（pinned+due+pending 入 /
pinned+due+error 入 / done 排 / pinned-no-due 排 / 非 pinned 排）/
按 due asc 排序 / header 含 "按 due 升序" 文案 / 边缘 case
（所有 pinned 无 due → 空兜底）。

## Key design decisions

- **active 含 Error**：error retry 时仍需关注 deadline；与 /blocked
  / /forks 含 error 同语义
- **按 due asc 排**：owner 关心"下一个 deadline 是哪条"— 最近到期
  在前；与 /due 按时段视图同节奏
- **无 N 截断 limit**：交集本身极少（典型几条 - 一二十条），不像
  /find 是模糊搜索需要 cap；truncate_if_overflow 是底线保护
- **不并入 /markers**：那个是 pinned + silent 双段；本命令是 pinned ×
  due 单段交集 — 维度不同不强行合并
- **不为「无 due 但 pinned」单独列段**：owner 想看 unfiltered pinned
  走 /pinned 即可；本命令专注 due-bearing 交集，多列段会模糊聚焦

## Verification

- `cargo test --lib telegram::commands::tests::pinned_due` — 6 / 6 通过
- `cargo test --lib`（全表）— 1445 / 1445 通过
- `npx tsc --noEmit`（frontend）— clean（无变更）
- `npx vite build`（frontend）— clean (1.24s)
