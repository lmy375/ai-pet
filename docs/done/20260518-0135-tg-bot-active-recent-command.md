# TG bot `/active_recent [N]` 命令（iter #441）

## Background

`/recent [N]` 显最近 N 条 done task（产出感 — 看「做完了什么」）；
`/oldest_n [N]` 显最老 N 条 pending（积压感 — 看「挂得最久」）。
但缺一条「最新创建的 active」入口 — owner 在 TG 上想扫读「我最近
塞了哪些活到队列」时只能走 `/last`（单条）或 `/tasks`（全表 +
compare_for_queue 智能排序，无法按 created_at 时序看）。

本 iter 加 `/active_recent [N]` — 反向 /recent done 的对偶；
- /recent done：updated_at desc 看「最新完成」
- /active_recent active：created_at desc 看「最新塞入」

## Changes

### `src-tauri/src/telegram/commands.rs`

#### 1. `TgCommand::ActiveRecent { n: u32 }` 变体

紧贴 `OldestN`（同 created_at 时序族）。

#### 2. 解析（与 /recent / /oldest_n 同模板）

```rust
"active_recent" => {
    let n = title
        .split_whitespace()
        .next()
        .and_then(|s| s.parse::<u32>().ok())
        .map(|n| n.clamp(1, 20))
        .unwrap_or(5);
    Some(TgCommand::ActiveRecent { n })
}
```

clamp 1..=20 缺省 5；非数字尾部走默认（与 /recent / /tasks since:7d
同前向兼容策略）。

#### 3. `format_active_recent_reply` pure 函数

```rust
let mut active: Vec<&_> = views.iter()
    .filter(|v| matches!(v.status, Pending | Error))
    .collect();
active.sort_by(|a, b| b.created_at.cmp(&a.created_at));  // desc
```

- 空 active → 「✨ 暂无 active 任务」兜底 + 教学
- 非空 → 标题行 `🆕 最近 N 条新建 active（共 M，按 created_at 降序）：`
  每行 `· MM-DD HH:MM · <emoji> <title> · N 天前`
- emoji 区分：🟢 pending / ⚠️ error
- 「N 天前」age label 仅 days >= 1 显（同日内省略）
- 超 N → overflow hint `…还有 K 条更早创建 active（用 /active_recent K 看更多，上限 20）`
- now 参数 inject 让单测稳定（与 /oldest_n 同 now-injection 模板）

### `src-tauri/src/telegram/bot.rs`

handler 紧贴 `OldestN`：

```rust
TgCommand::ActiveRecent { n } => {
    let views = read_tg_chat_task_views(chat_id.0);
    let now = chrono::Local::now().fixed_offset();
    crate::telegram::commands::format_active_recent_reply(&views, n, now)
}
```

复用 `read_tg_chat_task_views`（已 origin == Tg(chat_id) 过滤）+
`chrono::Local::now()` 本机时区算 age（与 /oldest_n 同模板）。

### Registry + ALL_HELP_TOPICS + help-for-topic + table line + 2x drift defense

- 双 lang registry 各加一条（紧贴 oldest_n）
- ALL_HELP_TOPICS 紧贴 "oldest_n"
- format_help_for_topic 加长详细文案（含与 /recent / /oldest_n / /last
  对比矩阵）；同步在 /oldest_n 详细文案末追加 /active_recent 交叉引用
- format_help_text 全表加 `/active_recent [N]` 一行
- 两处 drift-defense 测试列表加 "active_recent"

### 9 单元测试

parse（无参 / 显参 N / clamp 0/21/9999 / 非数字回默认）+ formatter（
空 active / 按 created_at desc 排序 / pending+error 含 / done+
cancelled 跳过 + emoji / N 截断 + overflow hint / age label `N 天前`）。

## Key design decisions

- **filter Pending + Error 非仅 Pending**：与 /oldest_n（仅 pending）
  不同。本命令是「按创建时序看最近塞入的活」 — error 仍是「活动轨
  道上的条目」（可 /retry 回 pending），创建时序意义上不该排除。/oldest_n
  仅 pending 是因为它语义偏「挂着没动」 —— error 状态偏「试过失败」
  非「没动」，与那个 lens 不匹配。两命令同源不同 lens
- **sort by created_at desc（非 updated_at desc）**：本命令名「active_recent」
  + 描述「最近 N 条新创建的 active」 — 用 created_at 才是「新塞入」语义。
  updated_at desc 用 /touch / /pri / /edit 都能拨动，混了「我刚操作过的」
  → 不是「新塞入」 lens。/recent 用 updated_at desc 是因为 done 的
  「完成时刻」就是 updated_at（done 后通常不再编辑），与本命令各自正确
- **clamp 1..=20**：与 /recent / /oldest_n / /digest / /alarms / /feedback_history
  / /recent_chats 全部 N-cap 命令统一上限。TG 单消息 4KB 限 + 屏读体验
- **status emoji 区分 🟢 / ⚠️**：active 含 pending + error 两态，emoji 让
  owner 一眼分辨「N 条新建里有几条是 error」 — 与 /tasks 同 emoji 矩阵
- **不显 priority / due / tags markers**：本命令是「时序扫读」 lens，
  markers 看 /show <title> 单点查；显 markers 让单行变臃肿且偏离 lens
- **不为 idempotent dedup**：每次都是「views snapshot 的最新 N」 — 没有
  状态变化要 dedup 的语义
- **不引 raw_description preview**：与 /recent 同（仅 title 不显内容）；
  想看 raw 走 /show <title>。本命令偏 audit 入口非详情入口
- **owner 主动写 created_at 时 stable**：created_at 是 ISO `YYYY-MM-DDThh:mm:ss±TZ`
  字典序 = 时间序（与 /recent / /oldest_n 同 sort 协议），无 parse 失败
  风险；formatter 直接 cmp(&v.created_at) 即可

## Verification

- `cargo test --lib telegram::commands::tests::active_recent` — 9 / 9 通过
- `cargo test --lib`（全表）— 1499 / 1499 通过（+9 from 1490）
- `npx tsc --noEmit`（frontend）— clean（无前端变更）
