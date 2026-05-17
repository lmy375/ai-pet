# TG bot `/swap_priority <a> :: <b>` 命令（iter #425）

## Background

`/pri <title> <N>` 单改 priority；owner 在 sprint 重组 / 优先级
交换场景常想「A 和 B 的优先级换一下」— 当前要：
1. 记住 A 的 P 值
2. /pri A <B 的 P>
3. /pri B <A 的 P>

三步 + 心算。本 iter 加 `/swap_priority <a> :: <b>` 一步完成 —
backend 自己读两 pre-swap priority 对称写两次。

## Changes

### `src-tauri/src/telegram/commands.rs`

#### 1. `TgCommand::SwapPriority { title_a, title_b }` 变体

紧贴 `Edit`（同 `::` separator pattern）。snake_case `swap_priority`
避开 dash drift-defense。

#### 2. 解析（first-occurrence `::` 切分）

```rust
"swap_priority" => {
    let (a, b) = match title.split_once("::") {
        Some((lhs, rhs)) => (lhs.trim().to_string(), rhs.trim().to_string()),
        None => (title, String::new()),
    };
    Some(TgCommand::SwapPriority { title_a: a, title_b: b })
}
```

#### 3. `format_swap_priority_reply` pure 函数

```rust
pub fn format_swap_priority_reply(
    title_a, title_b,
    pre_a: Option<u8>, pre_b: Option<u8>,
    save_a: Result<(), &str>, save_b: Result<(), &str>,
) -> String
```

5 态状态机：
- 任一 title 空 → usage hint（含 `::` 示例 + resolve 3-layer 说明）
- title_a == title_b → 「无需互换」兜底
- pre_a / pre_b 任一 None → 「task 不存在」 + 哪条没找到
- 全成功 → 「🔄 已互换 priority：「A」P3 → P7 · 「B」P7 → P3」
- 部分失败 → 「部分失败」+ 逐条 ✓ / ⚠️ 列出哪条 OK 哪条 fail

#### 4. Registry + ALL_HELP_TOPICS + help-for-topic + table line + 2x drift defense

- 双 lang registry 各加一条（紧贴 edit）
- ALL_HELP_TOPICS 紧贴 "pri / promote / demote" 集群
- format_help_for_topic 加详细文案
- /pri help 文案末追加 /swap_priority 交叉引用
- format_help_text 全表加 `/swap_priority <a> :: <b>` 一行
- 两处 drift defense 测试列表加 "swap_priority"

### `src-tauri/src/telegram/bot.rs`

handler 紧贴 `Edit`：

```rust
TgCommand::SwapPriority { title_a, title_b } => {
    if title_a.trim().is_empty() || title_b.trim().is_empty() {
        format_swap_priority_reply(&a, &b, None, None, Ok(()), Ok(()))
    } else {
        let actual_a = try_resolve_by_index(...).await
            .map(Ok).unwrap_or_else(|| resolve_tg_task_title(&title_a));
        let actual_b = ... same for b
        match (actual_a, actual_b) {
            (Ok(ta), Ok(tb)) => {
                let views = read_tg_chat_task_views(chat_id.0);
                let pri_a = views.iter().find(|v| v.title == ta).map(|v| v.priority);
                let pri_b = views.iter().find(|v| v.title == tb).map(|v| v.priority);
                if let (Some(a_val), Some(b_val)) = (pri_a, pri_b) {
                    let save_a = task_set_priority(ta.clone(), b_val);
                    let save_b = task_set_priority(tb.clone(), a_val);
                    format_swap_priority_reply(&ta, &tb, Some(a_val), Some(b_val),
                        save_a..., save_b...)
                } else {
                    format_swap_priority_reply(&ta, &tb, pri_a, pri_b, Ok(()), Ok(()))
                }
            }
            (Err(msg), _) | (_, Err(msg)) => format_command_error(&msg),
        }
    }
}
```

设计：
- **3 层 resolve 双 title 各自做**：与 /done / /cancel / /edit 同
  pattern；任一 fuzzy 失败显具体哪条候选不准
- **pre-swap 读 pri 一次**：read_tg_chat_task_views 拿 chat-scoped
  views，两 title 都从同一 snapshot 读 — 无 race condition（即便
  在两 task_set_priority 之间有并发改也按预期 pre-swap 值写）
- **对称写两次 task_set_priority**：复用既有单条改 backend，保留
  due / body / markers 不动；非事务但每条独立写盘失败容忍（per-step
  ok/err 报）
- **read priority 失败 = 「task 不存在」**：fuzzy resolve 命中后 views
  里找不到极端 case（task 在 resolve 与 read 之间被删）— 防御性
  兜底

### 7 单元测试

parse（双 `::` separator / 含空格 title / 缺 separator） + formatter
5 个场景（empty title / same title / missing pre / success / partial
failure）。

## Key design decisions

- **`::` 而非空格分隔**：与 /edit `::` separator 同协议；title 含
  空格 / 中文标点也精确切。owner 已熟悉 `::` 不引第二种 syntax
- **不做事务保证**：一次失败另一次仍执行 — 实际 task_set_priority
  失败极少（fs IO）；事务保证要引 backend 抽象不划算。部分失败时
  formatter 清晰报告
- **`a == b` 短路 noop**：避免无意义两次写盘 + 给 owner 明确反馈
- **三层 resolve 复用既有 helper**：与 /done / /cancel / /edit
  等同 pipeline，owner 熟悉 fuzzy 匹配规则
- **不引「revert」/「undo last swap」**：reroll 操作可手动再调
  /swap_priority 互换回去 — 与既有 /pri 同立刻可逆性

## Verification

- `cargo test --lib telegram::commands::tests::swap_priority` — 7 / 7 通过
- `cargo test --lib`（全表）— 1464 / 1464 通过
- `npx tsc --noEmit`（frontend）— clean（无变更）
- `npx vite build`（frontend）— clean (1.31s)
