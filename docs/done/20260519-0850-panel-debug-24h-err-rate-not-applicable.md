# PanelDebug 「📊 24h LLM 错误率」chip — backend 数据不支持，pivot drop（iter #549）

## Discovery

TODO 提案：「PanelDebug 「📊 24h LLM 错误率」chip：扫 llm.log 算 24h
内 error round 占比 — 既有进程级 err% 的 daily 窗口版」。

实际读 `write_llm_log` 实现（src/commands/debug.rs:279）发现 **llm.log
只记成功 round**：

```rust
fn write_llm_log(
    round: usize,
    request: &serde_json::Value,
    response_text: &str,      // ← 成功 round 的 response.text
    tool_calls: &[serde_json::Value],
    request_time: &str,
    first_token_time: Option<&str>,
    done_time: &str,
    first_token_latency_ms: Option<i64>,
    total_latency_ms: i64,
)
```

调用点（`commands/chat.rs:736`）仅在 LLM 成功回复后写。Error rounds
（network fail / API error / parse error / refusal）**不写 llm.log** —
没有 error 行可扫，没法算窗口化错误率。

既有「📊 err N%」chip（PanelDebug line ~2820）走 in-process atomic
counter `LlmOutcomeStats`（spoke / silent / error 三类），是**进程
启动以来累计**视角；不能按时间窗口切片，因为 counter 没时间戳。

## Decision

**不实现 24h LLM err% chip**。两条理由：

1. **数据不支持**：llm.log 只录成功；error 信息只在 atomic counter，
   没时间戳。要做窗口化 err% 需扩 logging 到「记 error rounds 含
   timestamp」— scope 大（多处 chat.rs 错误路径都要补写日志），且
   error log 噪音 / 隐私权衡需独立决策
2. **既有 chip 已够**：进程级 err% chip 已能信号化「pet 是不是异常多
   error」。pet 长期 alive owner 早会重启 / consolidate sweep 等都会
   隔出 session — process-wide 实质就接近 daily 视角

TODO 项删除，本 doc 作记录。procedure 教训（与 iter #516 /move_to 同
轨）：propose 涉及新 chip / 命令前，应 read 数据源实现确认它真有想算
的字段；表面命令名「llm.log scan」假定 log 含错误信息，实际不含。

## Future iters (out of scope)

- **背景 logging 扩**：让 chat.rs 各 error path 走 `write_llm_error_log`
  写带时间戳的 error row → 解锁所有「窗口化错误率」chip
- **noise vs privacy 决策**：error log 含 request body / API error msg
  可能携带 PII（owner prompt），需 redaction 策略
- **既有 chip 重设计**：把 atomic counter 改成 ring buffer（last N
  outcomes 含 ts）— 不引文件 IO 但 N 有限 / 重启丢
