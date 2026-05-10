# 删除隐私内容过滤相关代码

> 对应需求（来自 docs/TODO.md）：
> 删除关于隐私内容过滤的代码。注意要清理干净，同时保持代码规范。

## 范围

GOAL.md 已明确「不需要考虑隐私过滤相关的东西」。当前仓库内 redaction
（substring + regex 替换 → `(私人)`）层覆盖：proactive prompt 各类 hint、
工具返回（active window / calendar）、debug 统计、前端设置页 / 调试页。
逐项清理。

后端：

- 删除 `src-tauri/src/redaction.rs` 整文件
- `lib.rs` 删 `mod redaction;` 与 invoke_handler 中两条 redaction 命令
- `commands/settings.rs` 删 `PrivacyConfig.redaction_patterns / regex_patterns`，整个 `privacy` 字段保留但留空（避免破坏 yaml 兼容；后续如果完全不读再删）
- `commands/debug.rs` 删 `redaction_stats` 字段
- `proactive.rs` / `proactive/*.rs` / `tools/*.rs` 把所有 `crate::redaction::redact_with_settings(x)` 改为直接传 `x` / 拷贝
- `feedback_history.rs`、`task_heartbeat.rs` 同样

前端：

- `panelTypes.ts` 删 redaction 相关字段
- `PanelChipStrip.tsx` / `PanelDebug.tsx` 删 redaction stats 渲染段
- `useSettings.ts` / `PanelSettings.tsx` 删 `redaction_patterns` / `regex_patterns` 字段 + 输入 UI

## 非目标

- `MEMORY_PRIVATE_MARKER` 用户记忆里 (私人) 标签：那个是用户主动手写的便签语义，与"自动过滤"不同语义。检查后再决定是否动。
- `tool_review` 流程：与隐私过滤无关，保留。

## 风险

- 删后 prompt / 日志 / 工具输出会原文进入 LLM，与 GOAL.md 现状一致。
