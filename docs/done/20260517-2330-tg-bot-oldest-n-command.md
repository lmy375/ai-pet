# TG bot `/oldest_n [N]` 命令（iter #419）

## Background

`/recent [N]` 给 owner 看「最新 done」（产出感视图）— 反向「最
老 pending」（积压感视图）缺位。owner 想 audit「我哪些活儿挂得
最久 → 是否该重组优先级 / 砍掉」只能 /tasks 全列表 squint
created_at 字段比对，不直观。

本 iter 加 `/oldest_n [N]` 镜像反向，与 /recent 模板对偶。

## Changes

### `src-tauri/src/telegram/commands.rs`

#### 1. `TgCommand::OldestN { n: u32 }` 变体

紧贴 `Recent`，命名空间相邻。snake_case `oldest_n` 避开 dash
drift-defense。

#### 2. 解析

```rust
"oldest_n" => {
    let n = title.split_whitespace().next()
        .and_then(|s| s.parse::<u32>().ok())
        .map(|n| n.clamp(1, 20))
        .unwrap_or(5);
    Some(TgCommand::OldestN { n })
}
```

与 /recent 完全对称：缺省 5，clamp 1..=20，非数字尾走默认。

#### 3. `format_oldest_n_reply(views, n, now)` pure 函数

```rust
pub fn format_oldest_n_reply(views, n, now) -> String {
    let mut pending: Vec<_> = views.iter()
        .filter(|v| matches!(v.status, Pending))
        .collect();
    // created_at ISO 字典序 = 时间序，升序拿"最早创建在前"
    pending.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    if pending.is_empty() {
        return "✨ 本聊天派单暂无 pending...";
    }
    let take_n = (n as usize).max(1);
    let shown = &pending[..pending.len().min(take_n)];
    // 每行：MM-DD HH:MM · title · N 天前
    // overflow hint：还有 X 条更老 pending
}
```

设计：
- **仅 pending — error 不算**：error 在 active 池但属「试过失败」
  非「挂着没动」语义偏弱；/oldest_n 聚焦「真挂着的活」
- **created_at ISO 字典序升序 = 时间序升序**：task_queue 标准化为
  `YYYY-MM-DDTHH:MM:SS±TZ`，字典序与时间序一致 — 直接 cmp 不调
  parse 防错
- **「N 天前」相对 age label**：rfc3339 parse + days diff，0 天的
  不显（避免「0 天前」噪音），让 owner 一眼看「多老」
- **inject `now: DateTime<FixedOffset>`**：caller 传 chrono::Local::
  now().fixed_offset() — 让单测 inject fixed time 稳定 + age 算法
  与本机时区一致
- **MM-DD HH:MM 时间格式**：与 /recent 一致

### `src-tauri/src/telegram/bot.rs`

handler 紧贴 `Recent`：

```rust
TgCommand::OldestN { n } => {
    let views = read_tg_chat_task_views(chat_id.0);
    let now = chrono::Local::now().fixed_offset();
    format_oldest_n_reply(&views, n, now)
}
```

reuse recent 同 read path；formatter 内部 filter pending 让 handler
极简。

### Registry + ALL_HELP_TOPICS + help-for-topic + table line + drift defense

- 双 lang registry 各加一条（紧贴 recent）
- ALL_HELP_TOPICS 紧贴 `"recent"`
- format_help_for_topic 加 `"oldest_n" => "⌛ ..."` 长详细文案
- /recent help 文案末追加交叉引用 /oldest_n
- format_help_text 全表加 `/oldest_n [N]` 一行
- drift-defense 测试列表加 "oldest_n"

### 7 单元测试

parse（缺省 5 / 显式 N / clamp 50→20 / 0→1）+ formatter 5 个场景：
empty fallback / created_at asc sort / age label / non-pending
filter (error/done/cancelled 排除) / N truncate + overflow hint。

## Key design decisions

- **不取 error 入**：与 /buckets / /pinned_due / /forks 含 error
  反方向 — 那些是「优先级 / 解锁影响」audit，含 error 自然；本
  命令是「挂着没动」语义，error retry 状态不算「没动」
- **不显 priority 信息**：format_task_line 会让行变长；本命令焦点
  是「老」时间维度，priority 可走 /pri 单条调或 /buckets 看分布
- **手敲 N 是 N → clamp 而非 reject**：与 /recent / /digest 同模板
  — 容忍坏值给默认值降低 owner 学习成本
- **不为更老阈值参数化**：/oldest_n 5 已足够 audit；想看更全用
  /tasks 后 owner 自己挑 — 不堆 args
- **复用 created_at 字典序**：与 /pinned_due / /yesterday / /timeline
  同 string-compare 模式 — 一致心智

## Verification

- `cargo test --lib telegram::commands::tests::oldest_n` — 7 / 7 通过
- `cargo test --lib`（全表）— 1457 / 1457 通过
- `npx tsc --noEmit`（frontend）— clean（无变更）
- `npx vite build`（frontend）— clean (1.25s)
- 一处「ASCII 引号在 Rust 字符串字面量内」编译失败已修（与
  iter #371/#393/#411 同 fix — 用「」全角引号替换）
