# TG bot `/due <preset>` 命令（iter #304）

## Background

owner 在 TG 端已有 `/today` 看今日 due / done 任务，但缺"明天 / 本周 /
下周什么"的前向 audit。本迭代加 `/due <preset>` 命令扩展 /today 的时间
视角 —— preset = tomorrow / thisweek / nextweek（含中英 alias，缺省
tomorrow）。

## Changes

### `src-tauri/src/telegram/commands.rs`

- 新枚举 `DuePreset { Tomorrow, ThisWeek, NextWeek }` + 纯函数
  `parse_due_preset(&str) -> Option<DuePreset>`，识别中英 alias
  （tomorrow / tmr / tm / 明天 / 明日；thisweek / this-week / week / 本周 /
  这周；nextweek / next-week / 下周；大小写不敏感）
- enum `TgCommand` 加 `Due { preset: Option<DuePreset>, raw_arg: String }`
- `name()` → `"due"`；`title()` 归入 "" 桶（无 title 参数）
- 解析器："due" 分支：空 arg → `Some(Tomorrow)` + 空 raw；非空首 token
  parse；无法识别 → `preset=None` + 原 token 进 raw_arg
- 纯函数 `due_preset_range(preset, today) -> (start, end)` 把 preset 展
  开为 ISO 周日期范围（Tomorrow = 单日；ThisWeek = 本周一..周日；
  NextWeek = 下周一..周日）。pure / today 注入便于单测
- 纯函数 `format_due_reply(views, preset, raw_arg, today)`：
  - preset = None → usage hint 回显 raw_arg
  - preset = Some → 过滤 pending + due 日期落入 [start, end] 闭区间
  - 按 due 升序排（ISO 字典序 = 时间序）
  - 显头 "📅 明天 / 本周 / 下周（label）" + 行 "MM-DD HH:MM · title"
  - 空命中 → "该时段无 due 任务 ✨"
  - 溢出 10 条补 "…还有 N 条"
- registry zh + en 都加 ("due", desc) 在 ("today", ...) 之后
- `format_help_text` 全表加 `/due [preset]` 行（/today 之后）
- `format_help_for_topic` 加 "due" key + 把 "today" 详细 cross-reference
  加 "/due 更远视角"

### `src-tauri/src/telegram/bot.rs`

- 加 `TgCommand::Due { preset, raw_arg }` handler arm（在 Today arm 之
  后）：read_tg_chat_task_views + chrono::Local::now().date_naive() 注
  入 today，调 format_due_reply 一次性

### Tests

- 12 个新 unit test：
  - parser：默认 Tomorrow / 全空白 / 中英 alias 矩阵（3 preset × 各别名）
    / 未识别 preset 存 raw_arg
  - range：Tomorrow 单日 / ThisWeek 周中 + 周一边界 / NextWeek 跨周日
  - reply：未识别 preset usage hint 含 raw / Tomorrow 单日过滤 /
    ThisWeek 含已过工作日 / 排除 done + 无 due / 按 due 升序排
- 两个 drift-defense 名单 (`format_help_for_each_listed_command_returns_detail`
  + `tg_command_registry_covers_all_user_facing_commands`) 都加 "due"

## Key design decisions

- **preset 走 enum 而非自由字符串**：parser 时即 resolve 让无效输入早早
  落到 `None`，handler 路径不必再字符串 match。raw_arg 仍存让 usage hint
  能回显 owner 字面输入 ——「未识别 preset"lastweek"」比抽象错误信息更
  helpful。
- **ThisWeek 含已过工作日**：owner 在周四 audit "本周还剩什么"时，看到
  周一已 due 的项也是合理 signal（"哦，那条还没标 done 呢"）。如果只显
  未来部分会漏 audit 价值。formatter 不加 "已过" 标记 —— hits 按 due 升
  序排就让 owner 自然分辨。
- **due_preset_range pure + today 注入**：parser / handler 不持时间，
  formatter 接受 today 参数 —— 与既有 format_today_reply 同模板。让单
  测能固定 today=2026-05-14 进行确定性 range 断言。
- **不引入 yesterday / lastweek**：scope 控制 — owner 想看过去做了啥已
  有 /recent / /digest。/due 专注前向 audit。
- **ISO Mon..Sun 而非 Sun..Sat**：与 Rust chrono `weekday().num_days_from_monday()` 对齐 + 中国 / 大多数欧洲用户的"本周"直觉
  匹配（周日 = 周末末尾而非开头）。

## Verification

- `cargo test --lib`（backend）— 1129 passed / 0 failed（12 新 due 测试都
  通过；drift-defense 也命中新加的 "due"）
- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.19s)
