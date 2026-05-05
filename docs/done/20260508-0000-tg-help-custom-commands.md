# TG bot /help 自动列自定义命令 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> TG bot help 自动列自定义命令：现 `/help` 只列 5 条硬编码；改成读 settings.custom_commands 一并显示，让用户配完忘了输的命令名能被 /help 提醒。

## 目标

上一轮加了自定义命令配置 + TG 补全表注册。但 `/help` 命令的文案仍硬
编码 5 条 hardcoded —— 用户配完几条 custom 后忘了具体名字时，回不了 `/help`
查。本轮把 custom 列表也接到 `format_help_text` 末尾，让"我配过哪些命令"
的真相源单一。

## 非目标

- 不动硬编码 hardcoded 段的格式 / 顺序 —— 保持既有 5 条 + 紧迫 / 最紧迫
  说明行的视觉。
- 不去重（custom 不会与 hardcoded 冲突，因 `merged_command_registry`
  已过滤）。

## 设计

### 签名

`format_help_text(custom: &[TgCustomCommand]) -> String`：在文末注脚之
**前**插一段 "🛠 自定义命令：" + 每条 `/{name}  —  {description}`。

custom 为空时不渲染该段（保持原状）。

custom 用 `merged_command_registry` 同款过滤逻辑筛过的列表 —— bot.rs
handler 调时传 `&handler_state.custom_command_objects`（新增字段，与
custom_command_names 平行；或者直接现场从 settings 拿一遍）。

简化：handler 这层用 `state.custom_command_names` 已能拿名字，但缺
description。最干净是 HandlerState 加一个 `custom_command_full: Vec<TgCustomCommand>`
保存 merged 后的全集（剥 hardcoded 后的 custom 部分）。

### 测试

- `format_help_text(&[])`: 行为同原版（hardcoded 5 行 + 注脚）
- `format_help_text(&[cc("timer", "提醒")])`: 含 `/timer` 与 `提醒`
- 已有 `format_help_text_lists_all_commands_with_descriptions` 测试更新签名

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | format_help_text 加 custom 形参 + 渲染分支 + 单测 |
| **M2** | HandlerState 加 custom_commands 完整列表 + handler 接入 |
| **M3** | cargo test + tsc + build + cleanup |

## 复用清单

- 既有 `merged_command_registry` 过滤
- 既有 hardcoded help 文案
- 既有 `TgCustomCommand` struct

## 进度日志

- 2026-05-08 00:00 — 创建本文档；准备 M1。
- 2026-05-08 00:10 — M1 完成。`format_help_text(custom)` 形参化；非空时插 "🛠 自定义命令" 段；name/desc 任一空跳过；2 个新测覆盖 render section / skip blank entries；既有空 custom 测试更新签名。
- 2026-05-08 00:15 — M2 完成。HandlerState 加 `custom_command_objects` 字段（与 names 同源派生，避免重新校验）；Help 分支传 `&state.custom_command_objects`。
- 2026-05-08 00:20 — M3 完成。`cargo build` 10.46s 通过；`cargo test --lib` 976 通过（+2 新测）；`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过。归档至 done。
