# TG /cancel /retry 成功反馈附反向命令 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> TG `/cancel` 反馈消息附跳转链接：成功取消后追加一行 "如需恢复发 /retry <title>"；用户连续操作时不必再回 /help。

## 目标

`format_command_success("cancel", title)` 与 `format_command_success("retry", title)`
当前只确认动作。本轮在两段文案后追加一行**反向命令**指引：
- cancel 成功 → "如需恢复发 /retry <title>"
- retry 成功 → "如需取消发 /cancel <title>"

让用户在连续操作（误取消 → 立刻 retry / 重试失败 → 立刻 cancel）时不必回
`/help` 查命令名。

## 非目标

- 不做 deep-link / inline keyboard —— TG bot library 支持，但与既有 cancel/
  retry/tasks/help 全部走 plain text 风格，引入 inline keyboard 是"另一个项目"
  级别的改造。
- 不写 README —— TG 命令体验微调。

## 设计

`format_command_success` 在 cancel / retry 分支末尾各加一行：

```rust
"cancel" => format!(
    "🚫 已取消「{}」\n如需恢复发 /retry {}",
    title, title,
),
"retry" => format!(
    "🔄 已重置「{}」回 pending，下一轮宠物会重新尝试\n如需取消发 /cancel {}",
    title, title,
),
```

title 保留原 trim 后的字符串。如果 title 含空格（"整理 Downloads"），TG 用户
后续 `/retry 整理 Downloads` 会被 `parse_tg_command` 正确切出 cmd + arg。

### 测试

更新既有 2 条测试 + 加 2 条新断言（cancel / retry 都含 inverse cmd 字符串）。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | format_command_success cancel / retry 分支扩 + 测试 |
| **M2** | cargo test |

## 复用清单

- 既有 `format_command_success` 与 cancel/retry 调用方
- 既有 success_cancel / success_retry 单测

## 进度日志

- 2026-05-05 31:00 — 创建本文档；准备 M1。
- 2026-05-05 31:05 — 完成实现：
  - **M1**：`telegram/commands.rs::format_command_success` cancel / retry 分支末尾追加换行 + 反向命令指引（cancel → `/retry <title>`，retry → `/cancel <title>`）。`_` fallback 不动。既有 2 条单测扩 assert 反向命令字符串出现。
  - **M2**：`cargo test --lib` 898/898 通过。TODO 移除条目；本文件移入 `docs/done/`。
  - **README 不更新** —— TG 命令体验微调。
  - **未做手动 dev 验证**：当前会话不便启动 Tauri 桌面 app；纯字符串拼接 + 单测覆盖。
