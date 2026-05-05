# TG `/help` 命令 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> TG `/help` 命令：TG 用户输 `/help` 列出可用命令清单 + 简短示例，新人不必记 cancel / retry / tasks 各自语法。

## 目标

把现有的"未知命令"反馈（已经隐含列了 cancel / retry / tasks）升级成正式的
`/help` 命令。新人在 TG 输 `/help` 即可得到一份命令矩阵：每条 `/<name> [<arg>]
— <说明>` 一行 + 一条总注脚。

## 非目标

- 不做命令分组 / 翻页 —— 4 条命令直接列。
- 不做 inline keyboard / button —— teloxide 支持，但 plain text 已够，引入交互
  按钮反而和现有 cancel/retry/tasks 风格不一致。
- 不写 README —— TG 命令矩阵的完整化补强，与既有 cancel/retry/tasks/help
  系列同性质。

## 设计

### 解析

`telegram::commands::TgCommand` 加 variant `Help`（无参，与 Tasks 同形态）。
`parse_tg_command` 加 `"help" => Some(TgCommand::Help)` 分支。

### 文案

新 pure helper `format_help_text() -> String`：

```
🤖 可用命令：

/tasks  —  列出本会话派出的任务清单（按状态分组）
/cancel <title>  —  取消指定任务（无原因；详细原因请回桌面）
/retry <title>  —  把失败任务重置回 pending
/help  —  显示本帮助

💡 任务执行结果会自动通过本会话回传给你。
```

### handle_tg_command 接线

`bot.rs` 的 `handle_tg_command` 加 `Help` 分支 → `format_help_text()`。
`format_unknown_command` 文案改成"… 输入 /help 查看可用命令"，少冗余列举。

### 测试

- `parse_tg_command("/help")` → `Some(Help)`
- `parse_tg_command("/HELP")` → 大小写不敏感
- `parse_tg_command("/help anything")` → 仍命中 Help（与 /tasks 行为对偶，参数
  忽略）
- `format_help_text()` 含 `/tasks` / `/cancel` / `/retry` / `/help` 四条
- `format_unknown_command("foo")` 现在含 `/help`

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | parse + Help variant + format_help_text + 单测 |
| **M2** | bot.rs::handle_tg_command 加 Help 分支 + format_unknown_command 简化 |
| **M3** | cargo test + tsc + cleanup |

## 复用清单

- 现有 `TgCommand::Tasks` 是无参 variant 的样板
- `bot.rs::handle_tg_command` 现有 dispatch 模式
- 现有 telegram::commands::tests 的测试套

## 待用户裁定的开放问题

- `/help` 是否带本会话 chat_id 等元数据？本轮**否**——纯静态文案，复用
  pure helper 利于单测。

## 进度日志

- 2026-05-05 13:00 — 创建本文档；准备 M1。
- 2026-05-05 13:20 — 完成实现：
  - **M1**：`telegram/commands.rs` 加 `TgCommand::Help` variant；`parse_tg_command` 加 `"help" => Some(Help)` 分支（与 `/tasks` 同形态：尾参忽略）；`name()` / `title()` 接入 Help。新增 pure helper `format_help_text()` 输出 6-行命令矩阵 + 总注脚。`format_unknown_command` 收紧为单一指向 `/help`，避免在多处重复列举命令。新增 4 条单测覆盖 parse / 大小写 / 尾参忽略 / 文本完整性；修订 2 条历史用 `/help` 当 Unknown 样本的测试改用 `/zzznotacmd` / `/FoOBaR` 臆造名（`/help` 现已是正式命令）。
  - **M2**：`telegram/bot.rs::handle_tg_command` 加 `Help` 分支 → `format_help_text()`，import 同步引入。
  - **M3**：`cargo test --lib` 872/872（+4）；`pnpm tsc --noEmit` 干净；`pnpm build` 496 modules 全过。TODO 移除条目；本文件移入 `docs/done/`。
  - **README 不更新** —— TG 命令矩阵的完整化补强，与 cancel/retry/tasks 系列同性质。
  - **设计取舍**：`format_unknown_command` 收紧（不再列具体命令）—— 命令清单只在一处 (`format_help_text`) 维护，未来加新命令免漏改；`/help` 与 `/tasks` 同模式（无参 + 尾参忽略），保持 TG 命令族风格统一。
  - **未做手动 dev 验证**：当前会话不便启动 Tauri 桌面 app；解析与文本组装是 pure，单测覆盖完整，IO 路径仅一行 dispatch（与已有 cancel/retry/tasks 同模板）。
