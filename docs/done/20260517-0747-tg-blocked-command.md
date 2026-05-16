# TG bot `/blocked` 命令（iter #266）

## Background

TG bot 已有 `/today` / `/recent` / `/find` / `/pinned` / `/silenced` 等多种
查询切片，但缺少 "我哪些 task 卡住了 / 等什么" 这一关键维度。`[blockedBy:
...]` marker 用于表达依赖（"先做 A 再做 B"），proactive 选单已会自动隐藏
被卡的 task，但 owner 在外面想 audit "哪些 task 还等着 unblock"目前没专门
入口（要走 `/tasks` 看完整列表 + 自己识别）。

本迭代加 `/blocked` 命令：列出本 chat 派单中状态 = pending/error 且其
`[blockedBy: ...]` 仍有未解决项的 task，每条下方缩进列出仍卡在哪几条
blocker 上。模板与 `/recent` / `/find` 同（chat-scoped read + pure
formatter）。

## Changes

仅后端：

- `src-tauri/src/telegram/commands.rs`：
  - `TgCommand::Blocked` 新变体（无参）；`name()` / `title()` 同步
  - `parse_tg_command` 加 `"blocked"` 分支（多余尾部忽略，与 /tasks / /today
    同容忍策略）
  - `tg_command_registry_localized` zh+en 注册
  - `format_blocked_reply(views)`：
    - 算 active 集合 = `Pending | Error` 状态的 title
    - 对每条 view，过滤出 `blocked_by ∩ active`；非空且自身也 active 时
      添到 rows
    - 状态 emoji（🟢 pending / ⚠️ error）+ 标题 + 缩进 blocker 列表
    - 无命中 → "✅ 暂无被卡的 task" 友好文案
  - `format_help_text` 多加一行
  - 8 个新单元测试覆盖：parse / 空 views / 无 blocker / 单 blocker 命中 /
    blocker 已 done 视作已解决 / 自己 done 跳 / 多 blocker 部分命中（typo
    blocker 宽容跳过）/ error 状态也算被卡

- `src-tauri/src/telegram/bot.rs`：`TgCommand::Blocked` 分支调
  `format_blocked_reply`；views 走既有 `read_tg_chat_task_views(chat_id)`。

## Key design decisions

- **与 task_queue::unresolved_blockers 同算法但独立实现**：保持 formatter
  pure（不依赖 Vec<(String, String)> 元组形态）。复用 TaskView 的现成
  `blocked_by` 字段省一次 parse。
- **active = pending+error**：与 task_queue 的 unresolved_blockers 同语义
  （done / cancelled 视作"已解决"，typo / 已删 blocker 也视作已解决 — 宽容
  防永久卡死）。
- **chat-scoped views 当 active 集 = 近似**：精确做法是读全部 butler_tasks
  算 active 集；但 owner 在 TG /blocked 关心的是本 chat 派出的 task 链路，
  跨 chat blocker 是边缘场景。简化保持单一 read path 的一致性。
- **缩进 `└ 等：blocker` 表达从属关系**：单行不够；TG 客户端等宽字体下，
  缩进 + 树枝状 prefix 让 owner 一眼看清 task→blockers 的层级。
- **error 状态也算"被卡"**：error 表示宠物试过但失败 — 如果 blocker 还
  active，重试无意义；owner 应该先去解 blocker。`/blocked` 列出 error 让
  owner 不忘"这条还在等"。

## Verification

- `cargo check` ✅
- `cargo test`（含 8 新测试 + 全表 1053 通过）✅
- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.24s)
