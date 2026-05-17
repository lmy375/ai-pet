# TG bot `/buckets` 命令（iter #418）

## Background

`/stats` 给 owner 看 active task **状态维度**汇总（待办 / 逾期 /
今日完成 / 出错 / 今日取消）。**priority 维度**没有对应命令 —
owner 想"我各档高优各有几条" 只能 /tasks 看清单逐条 squint
priority。

本 iter 加 `/buckets` 镜像 priority 维度 dump — 与桌面 PanelTasks
priorityBands chip 同分组（P7+ / P5-6 / P3-4 / P1-2 / P0 五段），
一行式显计数。

## Changes

### `src-tauri/src/telegram/commands.rs`

#### 1. `TgCommand::Buckets` 变体（无参）

紧贴 `Stats`，命名空间靠近（自然 sibling）。

#### 2. 解析

```rust
"buckets" => Some(TgCommand::Buckets),
```

无参 + 多余尾部忽略（与 /stats / /pinned 同容忍）。

#### 3. `format_buckets_reply(views)` pure 函数

```rust
pub fn format_buckets_reply(views: &[TaskView]) -> String {
    let actives: Vec<_> = views.iter()
        .filter(|v| matches!(v.status, Pending | Error))
        .collect();
    if actives.is_empty() {
        return "🎯 本 chat 无 active task...";
    }
    let mut p7_plus = 0; let mut p5_6 = 0; let mut p3_4 = 0;
    let mut p1_2 = 0; let mut p0 = 0;
    for v in &actives {
        match v.priority {
            7..=u8::MAX => p7_plus += 1,
            5..=6 => p5_6 += 1,
            3..=4 => p3_4 += 1,
            1..=2 => p1_2 += 1,
            0 => p0 += 1,
        }
    }
    format!(
        "🎯 priority 分桶（{} 条 active）\nP7+: {} · P5-6: {} · P3-4: {} · P1-2: {} · P0: {}",
        actives.len(), p7_plus, p5_6, p3_4, p1_2, p0
    )
}
```

设计：
- **active 含 Error**：error retry 时仍需 priority 信号；与
  /forks / /blocked / /pinned_due 含 error 同语义
- **5 段分组**：与桌面 PanelTasks priorityBands chip / iter #392
  priority sparkline chip 一致 — owner 直觉转换无成本
- **P7+ 桶涵盖 7-9**：P7 / P8 / P9 都进；u8::MAX 上界容错
- **空兜底建议 /tasks**：与既有 /stats empty / /yesterday empty
  同模式

### `src-tauri/src/telegram/bot.rs`

handler 紧贴 `Stats`：

```rust
TgCommand::Buckets => {
    let views = read_tg_chat_task_views(chat_id.0);
    format_buckets_reply(&views)
}
```

reuse stats 同 read path；formatter 内部 filter active 让 handler
极简。

### Registry + ALL_HELP_TOPICS + help-for-topic + table line + drift defense

- 双 lang registry 各加一条（紧贴 stats）
- ALL_HELP_TOPICS 紧贴 "stats"
- format_help_for_topic 加 `"buckets" => "🎯 ..."` 长详细文案
- /stats help 文案末追加交叉引用 /buckets
- format_help_text 全表加 `/buckets` 一行
- 两处 drift-defense 测试列表加 "buckets"

### 5 单元测试

parse（无参 + case-insensitive + trailing 容忍）+ formatter 4 个
场景：empty fallback / 5 bands 覆盖 P0-P9 全 priority / active-only
filter（done/cancelled 排除）/ P7+ 涵盖 7-9 验证。

## Key design decisions

- **不显「全状态」分桶**：done / cancelled 不计 — 焦点是 active
  「待处理 priority 分布」。owner 想看历史 P 分布可走 /digest +
  目视计数
- **不显 P0 fold-in P1-2**：P0 单列保留 — P0 = "idea 抽屉" 与
  P1-2 = "低优但仍考虑" 语义不同，5 段映射保持
- **不带 N 参数 / 不分页**：5 段一行可读，不需要 cap
- **复用既有 PriorityBand 桌面分组节奏**：跨 surface 一致心智 —
  TG /buckets ↔ 桌面 priorityBands chip 同语义
- **handler 不做 active filter**：formatter 自带 filter → 单测
  可直 inject views 验证完整路径

## Verification

- `cargo test --lib telegram::commands::tests::buckets` — 5 / 5 通过
- `cargo test --lib`（全表）— 1450 / 1450 通过
- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.33s)
