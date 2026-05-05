# TG 长消息分页 (i/n) 前缀 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> TG 长消息分页 reaction：TG bot 当前消息超 4096 byte 自动 split，但缺少"第 1/3 部分"提示；接收方易把 split 后段误读为新一轮。

## 目标

`handle_message` 的回复路径在 `reply_text.len() > TELEGRAM_MSG_LIMIT` 时把回复
切成多块发送。当前每块都是独立消息文本，没有任何"上下文延续"提示——接收方
（特别是 TG 多人群组场景）容易把第 2、3 块误读为宠物又开了一次新话题。本轮
给每块前缀 `(i/n) `，让用户一眼分辨。

## 非目标

- 不改 split 边界算法 —— 现有"newline > space > byte 边界"启发式工作正常，
  本轮只在外层包前缀。
- 不做"在末尾加 `(待续...)`"—— 前缀已经携带 i/n 信息，加 suffix 噪音重复。
- 单块（短消息）不加前缀 —— `(1/1) ` 是噪音，仅 N≥2 时才有信号。
- 不写 README —— TG 体验微调。

## 设计

### 实现

在 `bot.rs` 加纯函数 `format_split_chunks(text, max_len) -> Vec<String>`：

```rust
const SPLIT_PREFIX_BUDGET: usize = 12; // 覆盖到 "(99/99) " = 8 chars + 安全垫

/// Pure：切超长消息成多块并加 `(i/n) ` 前缀。**调用前提** text.len() > max_len
/// （单块场景由调用方走快路径直接发原文，不加前缀）。
fn format_split_chunks(text: &str, max_len: usize) -> Vec<String> {
    let effective = max_len.saturating_sub(SPLIT_PREFIX_BUDGET).max(1);
    let chunks = split_message(text, effective);
    let n = chunks.len();
    chunks
        .iter()
        .enumerate()
        .map(|(i, c)| format!("({}/{}) {}", i + 1, n, c))
        .collect()
}
```

调用点（`handle_message`）：

```rust
if reply_text.len() <= TELEGRAM_MSG_LIMIT {
    bot.send_message(msg.chat.id, &reply_text).await?;
} else {
    for chunk in format_split_chunks(&reply_text, TELEGRAM_MSG_LIMIT) {
        bot.send_message(msg.chat.id, chunk).await?;
    }
}
```

`SPLIT_PREFIX_BUDGET = 12` 确保即便 N=99 分页，每块仍 ≤ 4096 byte（`split_message`
的 effective 上限是 4084，+ 前缀 ~8 char = 4092，安全垫 ~4 byte）。99 块 × 4084
= 400 KB 已远超实战 LLM 单回复，N 上限不需硬限。

### 测试

`format_split_chunks` 全 pure，单测：
- 短文本（< max_len）—— **不调用**这个函数；测调用方应该走快路径，也写一条测试
  确认 split_message 自身在小文本上行为合理（已是隐式契约）。
- 多块切分：每块带正确 `(i/n)` 前缀
- 每块 + 前缀 ≤ max_len（核心安全保证）
- 内容连续性：去掉前缀后拼起来 == 原文（split_message 的边界 trim 行为不破坏内容）
- 边界 N=2（最小分页场景）/ 大 N（30+ 块）

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | `format_split_chunks` 纯函数 + 单测 |
| **M2** | `handle_message` 接线 |
| **M3** | cargo test + cleanup |

## 复用清单

- 现有 `split_message` 边界启发式
- `TELEGRAM_MSG_LIMIT = 4096`

## 待用户裁定的开放问题

- 前缀格式：`(1/3) ` vs `[1/3]` vs `第1/3部分`？本轮选 `(1/3) `（最短、最国际、
  TG 渲染不易冲突 markdown 标记）。

## 进度日志

- 2026-05-05 19:00 — 创建本文档；准备 M1。
- 2026-05-05 19:20 — 完成实现：
  - **M1**：`telegram/bot.rs` 加 `SPLIT_PREFIX_BUDGET = 12` 常量 + `format_split_chunks(text, max_len)` 纯函数。effective budget = `max_len - SPLIT_PREFIX_BUDGET`（覆盖 `(99/99) ` + 安全垫），调既有 `split_message` 切块后给每块加 `(i/n) ` 前缀。`saturating_sub + max(1)` 防 max_len 极小时 panic。4 条新增单测覆盖：两块场景 prefix 与 ≤max_len 双校验、N>2 时索引正确递进、剥前缀拼接 == 原文（验证内容连续性）、极小 max_len 不 panic。
  - **M2**：`handle_message` 把 `split_message` 调用换成 `format_split_chunks`，同时保留单消息快路径不加前缀。
  - **M3**：`cargo test --lib` 879/879（+4）通过。TODO 移除条目；本文件移入 `docs/done/`。
  - **README 不更新** —— TG 体验微调，与 cancel/retry/tasks/help 系列同性质。
  - **设计取舍**：从 effective budget 里扣前缀预算（而非两遍 split 精算）—— 12 byte 损失对 4096 上限可忽略，省去了"先切看 N 再决定 prefix 长度"的递归；前缀 `(i/n) ` 而非 `[1/3]` / `第1/3部分` —— 最短、最国际、不与 markdown 冲突。
  - **未做手动 dev 验证**：当前会话不便启动 Tauri 桌面 app；逻辑全 pure，4 条单测覆盖核心契约（前缀正确 / 长度安全 / 内容不丢 / 极端边界）。
