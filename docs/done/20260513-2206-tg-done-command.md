# Telegram /done 命令

## 背景

TG bot 已有 `/cancel`、`/retry` 让手机端用户管理失败 / 卡住的任务，但**缺 /done** —— 已完成的任务想标 done 只能回桌面板。日常 butler_tasks 完成通过 LLM 自动 `[done]` 标，但手动派的任务 / 用户自己执行了的任务希望 TG 一键标。

## 改动

### `src-tauri/src/telegram/commands.rs`

1. **`TgCommand::Done { title }` variant** —— 加入 enum
2. `TgCommand::name() / title()` 加 done 分支（title accessor 与 Cancel / Retry 合并到 or-pattern）
3. `parse_tg_command("done", ...)` 分支 → `TgCommand::Done`
4. `tg_command_registry_localized` zh / en 双语都加 `("done", "...")` 行（注意位置：dispatch group 内，task / tasks 之后、cancel 之前 — 高频度按"创建 → 状态 → 失败处理"排序）
5. `format_command_success("done", title)` —— 反馈"✓ 已标 done「title」+ 暗示 result 需回桌面"
6. `format_help_text` —— 把原 `/cancel / /retry` 单行合并成 `/done / /cancel / /retry` 三参操作
7. 新单测：parse_done_with_title / parse_done_empty_title / done_command_name_and_title / format_done_success_includes_panel_hint
8. 更新 `tg_command_registry_covers_all_user_facing_commands` 断言新增 `"done"`

### `src-tauri/src/telegram/bot.rs`

1. missing-argument 分支扩展 or-pattern 包含 `TgCommand::Done`
2. 新增 `TgCommand::Done { title }` 处理分支，模式与 /cancel /retry 一致：
   - 三层 resolve（index → fuzzy → 错误带候选）
   - 调 `task_mark_done_inner(title, None, decisions)` —— TG 不收 result 摘要
   - 成功反馈走 `format_command_success("done", t)`

## 不做

- 不支持 `/done <title> <result>` 多参解析 —— TG 单行命令对中文 result 不友好；想加 result 走桌面板
- 不动 task_mark_done_inner 后端逻辑（已是 idempotent / 终态拒绝）

## 验收

- `cargo build --release` ✅
- `cargo test --lib` ✅ 全 **889 通过**（原 885 + 4 新）
- TG 端 `/done <title>` 或 `/done <序号>` 工作
- 已 done / cancelled 任务被后端拒，错误信息回传到 TG

## 完成

- [x] TgCommand::Done 加 enum + 字段 accessor
- [x] parse_tg_command 分支
- [x] tg_command_registry zh/en 双语
- [x] format_command_success done 分支
- [x] format_help_text 三参合并行
- [x] bot.rs missing-arg + Done 处理分支
- [x] 4 新单测 + registry 测试断言更新
- [x] 移到 docs/done/
