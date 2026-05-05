# TG 任务推送批量合并 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> TG 任务推送批量合并：watcher 现在逐条任务发完成 / 失败消息；高并发完成 N 条会刷屏。1 分钟窗口内同状态多条合并成一条 "✅ 已完成 3 条：A / B / C"，减少 TG 通知打扰。

## 目标

`run_task_watcher` 每 60s 扫一次 butler_tasks，对所有 just_finished 转移
分别发 TG 消息。如果一轮里宠物连完成 4 条任务，TG 收到 4 条独立通知刷屏。
本轮把同一 (chat_id, status) 的多条事件合并成一条 batch 消息，减少打扰。

## 非目标

- 不跨 tick 合并 —— 60s 已经足够小窗口；跨 tick 缓冲会引入 "上次没说完的
  通知现在才到" 的延迟感，得不偿失。
- 不动心跳通知 —— 心跳是单任务节奏的提醒（"X 卡 30 分钟了"），合并成
  "Y/Z 都卡了" 反而稀释信号；心跳本就少，单条噪音可控。
- 不动单条 message 的文案格式 —— 单事件保留既有 `format_completion_message`
  输出，只在事件数 ≥ 2 时走新的 batch 文案。

## 设计

### 数据收集

watcher 主循环里把 `if just_finished` 的分支改成：先把事件 push 到本轮的
`Vec<CompletionEvent>`，循环结束后按 `(chat_id, status)` 分组发送。

```rust
struct CompletionEvent {
    chat_id: i64,
    title: String,
    status: TaskStatus,
    reason: Option<String>,
}
```

### 分组策略

- 同 (chat_id, status) 的事件归为一组
- 单事件组 → 走旧的 `format_completion_message`，文案不变（防回归）
- 多事件组 → 新的 `format_completion_batch`

不同 status 不合并（混合 done + error 在同一行让用户难判断"是不是真的全
完成了"）。同 chat 多 status 各发一条。

### 文案（pure，bot.rs）

```rust
fn format_completion_batch(
    status: TaskStatus,
    titles_with_reasons: &[(String, Option<String>)],
) -> String;
```

格式：
- done：`✅ 已完成 {N} 条：A · B · C`
- error：`⚠️ 任务失败 {N} 条：A（{reason}）· B（{reason}）` —— reason 缺失
  时省略括号
- cancelled：`🚫 已取消 {N} 条：A · B · C`

间隔符 `·` 而非 `,` —— 中文标题里逗号常见，分隔符冲突；`·` 视觉清晰。

### 顺序

事件按"watcher 在 cat.items 里看到的顺序"输出。cat.items 是 YAML 写入序，
对用户而言"先完成的先列"通常符合直觉。无需额外排序。

### 测试

`format_completion_batch` 是 pure；单测覆盖：
- done 1 / 2 / 3 条
- error 带 reason / 不带 reason
- cancelled 多条
- 标题含特殊字符（emoji / 中文符号）原样保留
- N=0 不会被调用（防御性测试不写：调用方已保证 ≥ 1）

watcher 主循环改动是 IO，不写新单测；现有 `just_finished` 单测覆盖语义不变。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | `format_completion_batch` 纯函数 + 单测 |
| **M2** | watcher 主循环：收集 → 按 (chat_id, status) 分组 → 单/多分支发送 |
| **M3** | cargo test + tsc + build + cleanup |

## 复用清单

- 既有 `just_finished` 状态判定
- 既有 `format_completion_message` 单条文案（多条复用其文案 spirit）
- 既有 `bot.send_message`

## 进度日志

- 2026-05-06 21:00 — 创建本文档；准备 M1。
- 2026-05-06 21:10 — M1 完成。`format_completion_batch` 纯函数 + 6 个单测：done 多条 / error 带 reason 括号 / 空白 reason 不显示空括号 / done 忽略 reason / cancelled 多条 / 标题 emoji 中文符号原样保留。
- 2026-05-06 21:20 — M2 完成。watcher 主循环改成两阶段：循环内只 collect 到 `HashMap<(chat_id, status), Vec<(title, reason)>>`，循环后按 group 分发；单事件组走旧 `format_completion_message`（防回归），多事件组走新 batch 文案。给 `TaskStatus` 加 `Hash` derive 以支持 HashMap 键。
- 2026-05-06 21:25 — M3 完成。`cargo test --lib` 939 通过（含新增 6 测）；`cargo build` 7.83s 通过；`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过。归档至 done。
