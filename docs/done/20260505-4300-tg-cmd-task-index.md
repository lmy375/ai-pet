# TG 命令带任务编号支持 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> TG 命令带任务编号支持：`/cancel 1` / `/retry 2` 也接受最近一次 `/tasks` 输出里的序号（基于 chat 内 last_tasks_response 的对应顺序），不必键入 title 整段。

## 目标

TG 长 title（如"整理 Downloads & 备份"）在手机键盘里全键入烦。本轮让 `/cancel`
`/retry` 接受 1-indexed 整数 → 解析为最近一次 `/tasks` 输出列表的对应 title，
省去整段重打。

非数字 query / 数字越界 / 缓存空 → 自动 fall back 到既有 fuzzy resolve（精确
+ substring + 建议），不破坏既有路径。

## 非目标

- 不在 `/tasks` 输出里渲染序号 —— 现 list 形式（emoji + Pn + title）已紧凑，
  加序号会让每行变长。用户记得"上次第几条"足够；记不住可重发 /tasks 看顺序。
- 不持久化跨进程缓存 —— bot 重启 = 缓存清空，用户重发 /tasks 即可。
- 不写 README —— TG 命令体验补强。

## 设计

### Pure 解析

`telegram/commands.rs` 加：

```rust
/// 1-indexed 编号 → titles[N-1]。query trim 后非纯数字 / 数字 0 / 越界 → None。
/// 让 caller fall back 到 fuzzy resolve。
pub fn resolve_index_to_title(query: &str, titles: &[String]) -> Option<String> {
    let n: usize = query.trim().parse().ok()?;
    if n == 0 {
        return None;
    }
    titles.get(n - 1).cloned()
}
```

### bot.rs 状态

`HandlerState` 新加 `last_tasks_titles: TokioMutex<HashMap<i64, Vec<String>>>`
（chat_id → 上次 /tasks 输出顺序的 title 列表）。

`format_tasks_for_chat` 改签名为 `(String, Vec<String>)`，body + 显示顺序
titles。显示顺序 = `format_tasks_list` 的 section 顺序（Pending → Done →
Error → Cancelled），section 内沿用 `compare_for_queue` 排序。

Tasks dispatch 把 titles 缓存（即便 body dedup 命中"无变化"也更新 titles ——
确保后续 /cancel N 用最新顺序）。

### Cancel / Retry 接线

新加 async helper：
```rust
async fn try_resolve_by_index(query: &str, chat_id: i64, state: &Arc<HandlerState>) -> Option<String> {
    let cache = state.last_tasks_titles.lock().await;
    let titles = cache.get(&chat_id)?;
    resolve_index_to_title(query, titles)
}
```

Cancel / Retry 分支先 try numeric resolve，None → fuzzy resolve（既有路径）。

## 测试

- `resolve_index_to_title` 各边界（空 / 非数字 / 0 / 越界 / 精确 1-indexed）
- 集成路径（bot.rs）不写测试，与既有 cancel/retry IO 同等级"成本不值"

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | `resolve_index_to_title` pure 函数 + 单测 |
| **M2** | bot.rs `last_tasks_titles` state + format_tasks_for_chat 改签名 + Tasks dispatch 更新 titles |
| **M3** | Cancel / Retry 分支接 try_resolve_by_index → fuzzy fallback |
| **M4** | cargo test + cleanup |

## 复用清单

- 既有 `last_tasks_response` 缓存路径（同 lock pattern）
- 既有 `resolve_tg_task_title` fuzzy fallback
- 既有 `format_tasks_list` 不动

## 进度日志

- 2026-05-05 43:00 — 创建本文档；准备 M1。
- 2026-05-05 43:20 — 完成实现：
  - **M1**：`telegram/commands.rs::resolve_index_to_title(query, titles)` pure 函数（trim → parse usize → 0/越界 → None；非数字 → None；valid 1-indexed → Some(titles[N-1])）。6 条新增单测覆盖空 / 非数字 / 0 / 越界 / 1-indexed 三档 / trim / 空 titles。
  - **M2**：`bot.rs::HandlerState` 加 `last_tasks_titles: TokioMutex<HashMap<i64, Vec<String>>>`；`format_tasks_for_chat` 改签名为 `(String, Vec<String>)`，按 `format_tasks_list` 的 section 顺序（Pending → Done → Error → Cancelled）累积 title vec。Tasks dispatch 总是更新 titles 缓存（即便 body dedup 命中"无变化"）。
  - **M3**：加 `try_resolve_by_index(query, chat_id, state) -> Option<String>` async helper（锁 + 委托 pure 函数）；Cancel / Retry 分支三层 resolve（数字 → fuzzy → 错误反馈带建议），数字优先级最高。
  - **M4**：`cargo test --lib` 911/911（+6）通过。TODO 移除条目；本文件移入 `docs/done/`。
  - **README 不更新** —— TG 命令体验补强。
  - **设计取舍**：数字 resolve 在 fuzzy 之前（用户键入纯数字时意图最明确）；缓存进程内（重启 = 重发 /tasks），不持久化保持简单；不在 /tasks 输出渲染序号（list 已紧凑，加序号让每行变长，用户记得"上次第 N 条"足够）。
  - **未做手动 dev 验证**：当前会话不便启动 Tauri 桌面 app；解析层 6 条单测覆盖边界，IO 层是 Mutex lock + 既有 inner 调用。
