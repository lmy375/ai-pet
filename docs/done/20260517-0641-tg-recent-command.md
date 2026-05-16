# TG bot `/recent [N]` 命令（iter #260）

## Background

TG bot 已有 `/today`（今日到期 + 已完成两段）/ `/tasks`（全部状态分组）/
`/stats`（数字汇总）。owner 在外面想"我最近做完了什么"扫读 — 比 /today 更
宽（不限今日，能看到昨天 / 前天的完成）；比 /tasks 聚焦（只 done 段）。

本迭代加 `/recent [N]`：返回本 chat 派单中最近 N 条 done 任务标题清单，按
`updated_at` 倒序。N 缺省 5，clamp `1..=20`，非数字尾部容忍走默认（与
/tasks since:7d 同前向兼容策略）。

## Changes

仅后端：

- `src-tauri/src/telegram/commands.rs`：
  - `TgCommand::Recent { n: u32 }` 新变体；`name()` / `title()` 同步
  - `parse_tg_command` 加 `"recent"` 分支：取首个 whitespace token 试 parse
    `u32`，clamp `1..=20`，失败 fallback 5
  - `tg_command_registry_localized` 中英两份注册（在 today / reset 之间）
  - `format_recent_reply(views, n)` 函数：filter done + sort updated_at
    desc + take n + 头部 `✅ 最近 X 条完成（共 Y）` + 每行 `· MM-DD HH:MM ·
    title` + 末尾 `…还有 K 条更早完成（用 /recent N 看更多，上限 20）`
  - `format_help_text` 多加一行 `/recent [N] —— 最近 N 条已完成任务标题`
  - 8 个新单元测试覆盖：默认 5 / 显式 N / clamp 0→1 + 21→20 / 非数字
    fallback / 空 done 段文案 / 倒序 / 跳过非 done 状态 / 截断 + 溢出 hint

- `src-tauri/src/telegram/bot.rs`：`TgCommand::Recent { n }` 分支调
  `format_recent_reply(&views, n)`；views 来自既有
  `read_tg_chat_task_views(chat_id)` （已 origin==Tg(chat_id) 过滤）。

## Key design decisions

- **复用既有 read path**：`read_tg_chat_task_views` 已实现 chat-scoped
  filter，与 /tasks / /today / /stats 同源。不引入新 IPC，避免 chat 跨范围
  漂移。
- **clamp 1..=20**：N=0 无意义；N>20 让单条 TG 消息撑爆 4096 字符上限风险。
  20 条标题大约 1500-2500 字符，安全留白。
- **倒序 by updated_at**：done 状态切换时 `updated_at` 被刷，作完成时间
  代理。ISO `YYYY-MM-DDThh:mm[:ss]±TZ` 字典序与时间序一致，简单 string
  compare 够用。
- **格式 `MM-DD HH:MM`**：full ISO 太长占行；`MM-DD HH:MM` 5+5 = 11 字符
  + 两个 `·` 分隔符，单行紧凑且含日期 + 时刻精度足够 owner 回忆。
- **非数字 fallback 5 而非 Unknown**：用户随手 `/recent abc` 或 `/recent
  please` 不报错走默认更友好；明确错误信号只对解析层"我不知道这是啥命令"
  保留。

## Verification

- `cargo check` ✅
- `cargo test`（含 8 新测试 + 全表 1038 通过）✅
- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.22s)
