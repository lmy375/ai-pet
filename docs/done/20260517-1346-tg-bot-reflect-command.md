# TG bot `/reflect <text>` 命令（iter #302）

## Background

owner 在 TG 端已有 `/note <text>` 把任意文本作 general memory item 存
（杂项 brain-dump）。但反思 / 自我观察类内容（"今天回顾：..." / "观察：
长 task 拆细后..."）混进 general 后让 PanelMemory → AI 洞察 段无法成
为"宠物的真正反思流"。

本迭代加 `/reflect <text>` 命令 —— 与 /note 同写入模板但 category =
ai_insights，按信号类型分流让 ai_insights 段保持反思 / 洞察纯度。

## Changes

### `src-tauri/src/telegram/commands.rs`

- enum `TgCommand` 加 `Reflect { text: String }` 变体（与 Note 同结构）
- `name()` → `"reflect"`；`title()` → `text.as_str()`
- 解析器：`"reflect" => Some(TgCommand::Reflect { text: title })`，与 /note
  同模板（含空格保留 / 空 text 由 handler 处理）
- `format_reflect_reply(text, save_result)` 纯文案 formatter：
  - 空 → usage hint（含与 /note 的对比例 + ai_insights 分类说明）
  - Ok(title) → "🪞 已记到 ai_insights/<title>" + 60 字预览
  - Err(msg) → "🪞 保存失败：<msg>"
  - 🪞 镜子 emoji 对应 "反思" 语义；与 /note 的 📝 笔记 emoji 区分
- `tg_command_registry_localized` zh + en 都加 ("reflect", desc)
- `format_help_text` 全表加 `/reflect ...` 行（/note 之后）
- `format_help_for_topic` 加 "reflect" key 含详细用法 + 示例 + 与 /note
  对比说明
- drift-defense `format_help_for_each_listed_command_returns_detail` +
  `tg_command_registry_covers_all_user_facing_commands` 都加 "reflect"
- 6 个新 unit test 覆盖：parse 文本 / parse 空 text / usage hint 含 /note
  对比 / 成功 preview / 长 text 截断 / 失败错误反馈

### `src-tauri/src/telegram/bot.rs`

- 加 `TgCommand::Reflect { text }` handler arm（在 Note arm 之前）：
  - 空 text → `format_reflect_reply` 走 usage hint
  - 否则 title 自动 `reflect-YYYY-MM-DDTHH-MM-SS`（与 /note 同模板）
  - `memory_edit("create", "ai_insights", title, Some(trimmed), None)`
  - 成功 / 失败分支都走 `format_reflect_reply` 文案

## Key design decisions

- **category 分流而非合并 + tag**：ai_insights / general 两类目语义本就
  不同（一个是宠物的反思流 / 一个是 owner 的杂项 brain-dump），用专用
  命令而非 `/note --type=reflect` 这类 flag 让 TG slash autocomplete 浮
  出独立入口，owner 不必记 flag 语法。
- **复用 memory_edit 完整副作用链**：与 /note 同走 memory_edit("create")
  入口；ai_insights 不走 SQLite mirror（与 butler_tasks / todo /
  task_archive 不同），但其它副作用（detail dir 创建 / index 写盘 /
  PanelMemory live update）都跟进。
- **title prefix "reflect-" 而非 "note-"**：title 命名空间分流给后续
  audit / 整理 tooling 留辨识度。owner 在 PanelMemory 看 ai_insights 段
  时看到 reflect-… 一目了然"这是 TG 端来的"。
- **drift-defense 测试同步双名单**：本迭代加 reflect 到
  format_help_for_each_listed_command_returns_detail 和
  tg_command_registry_covers_all_user_facing_commands 两个 drift-defense
  test 名单。/note 历史上没加入 drift list 是 pre-existing 缺口，本迭代
  不扩 scope；/reflect 起点完整。
- **usage hint 显含 /note 对比**：让 owner 知道有两个入口、按信号类型
  分流。避免 owner 用错（"我应该 /note 还是 /reflect？" → reply 文案先
  教会决策）。

## Verification

- `cargo test --lib`（backend）— 1117 passed / 0 failed（6 新 reflect
  测试都通过；drift-defense 也命中新加的 "reflect"）
- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.24s)
