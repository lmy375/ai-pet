# TG bot `/help <cmd>` 单命令详细帮助（iter #278）

## Background

`/help` 当前一次性列全表（每命令一行 + 一行描述），共 25 行 — TG 屏适用但
单条命令的细节（用法 / 示例 / 注意事项）owner 还得自己脑补。新用户 / 偶尔
用 TG 的 owner 想知道"/snooze 有哪些 preset" 没专门入口。

本迭代加 `/help <cmd>`：传命令名作 topic，回多行详细段（用法 / 示例 / 相关
命令）。25 条内置命令全覆盖；custom 命令命中显 owner 自配的 description；
未命中走友好兜底。

## Changes

仅后端：

- `src-tauri/src/telegram/commands.rs`：
  - `TgCommand::Help` → `TgCommand::Help { topic: Option<String> }`
  - 同步更新 `name() / title()` 方法 + parser 分支
  - 新增 `format_help_for_topic(topic, custom)` pure 函数：
    - 25 条内置命令 hardcoded match arm → 多行详细段（用法 / 示例 / 相关）
    - topic 可带 `/` 前缀，trim + lowercase
    - 空 topic → fallback 到 `format_help_text`（与无参 /help 一致）
    - custom 命中 → 显 `🛠 /xxx（自定义命令）\n\n description`
    - 未命中 → 友好兜底 `❓ 未知命令「/xxx」。发 /help 看完整命令表。`
  - 9 个新单元测试覆盖：parse no-topic / case-insensitive / with-topic /
    strip `/` 前缀 / case-insensitive topic / unknown friendly / custom
    fallback / empty falls back / 全表 25 条都有详细文案（防 drift）

- `src-tauri/src/telegram/bot.rs`：`Help { topic }` 分支按 Some/None 分流
  调 `format_help_for_topic` 或既有 `format_help_text`。

## Key design decisions

- **hardcoded match arms 而非 table-driven**：单命令详细文案信息量大（用法 +
  示例 + 注意），用 table 表达字段太多 / 嵌套多。match arms 写起来啰嗦但
  阅读 / 维护时直接 — 改一条命令文案就改一处。
- **drift 防御测试**：`format_help_for_each_listed_command_returns_detail`
  遍历 25 条内置命令名，断言每条都有详细文案（"用法" 子串）。新增命令时
  忘加 detail 会 fail，强制开发者补上。
- **`/` 前缀允许**：owner 习惯 `/help cancel` 也可能 `/help /cancel`；trim
  `/` 前缀让两种都生效。
- **空 topic → 全表 fallback**：parser 已经把空 title 映射成 None；但
  format_help_for_topic 也加同样的兜底（防 caller 误传空字符串）。
- **custom 命令显 description**：详细用法 owner 自己最清楚；显他配的
  description 比"不识别"友好。

## Verification

- `cargo check` ✅
- `cargo test`（含 9 新 help 测试 + 全表 1073 通过）✅
- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.25s)
