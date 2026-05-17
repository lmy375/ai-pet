# TG bot `/note <text>` 命令（iter #286）

## Background

owner 在外面常有"想到一件事 / 想法 / 待回看"的随手想 dump 到宠物 memory
的诉求。当前路径：要么 `/task <title>` 创建任务（但很多 dump 不是任务 —
就是个想法 / 笔记），要么 chat 与宠物说但会被 LLM 直接消化 + 可能丢到非
预期的 memory cat。

本迭代加 `/note <text>` 命令：把任意一段文本作 general memory item 存盘。
title 自动按本地时间生成（秒级唯一防撞）；description = 用户原文 trim。

## Changes

仅后端：

- `src-tauri/src/telegram/commands.rs`：
  - `TgCommand::Note { text: String }` 新变体；`name() / title()` 同步
  - `parse_tg_command` 加 `"note"` 分支：所有 arg 当 text 保留（含空格）
  - registry zh+en 注册（在 /mute 之后）
  - `format_note_reply(text, save_result)` pure：
    - 空 / 全空白 text → usage hint + 例子
    - `Ok(title)` → "📝 已记到 general/{title}\n\n{preview}"；preview 截 60 字符
    - `Err(msg)` → "📝 保存失败：{msg}"
  - help text 多加一行
  - 7 个新单测覆盖：parse text / parse 空 / 空 reply 走 usage / 全空白
    走 usage / 成功 reply 显 title + preview / 长文本 preview 截断 /
    保存失败显错

- `src-tauri/src/telegram/bot.rs`：`Note { text }` 分支：
  - trim 空 → format_note_reply(text, Ok(""))（让 formatter 内部走
    usage hint 路径）
  - 否则 title = `note-YYYY-MM-DDTHH-MM-SS`（chrono::Local::now）
  - 直接调 `crate::commands::memory::memory_edit("create", "general", ...)`
    — 与桌面 PanelMemory "新建 general item" 同后端
  - 走 format_note_reply 反馈结果

## Key design decisions

- **存到 general 而非 ai_insights**：general 是兜底 / 杂项类目，最贴"random
  thought" 语义。ai_insights 是宠物自己的反思 / persona 长期画像，owner
  随手记不该污染。
- **title 用时间戳而非内容摘要**：内容摘要可能重复 / 含 newline / 触发 markdown
  解析 footgun；时间戳秒精度天然唯一、可排序。owner 进 PanelMemory 看
  general 段时按时间序自然成"日记式" log。
- **preview 60 字 + …**：TG 单条消息有 4096 字符上限；同时让 owner 在
  发完后立即看到"我刚记了啥前 60 字"做快速确认。完整文本进 description
  field（PanelMemory 看 / 进 detail.md 编辑）。
- **复用 memory_edit("create", ...)**：与桌面 PanelMemory "新建" 同后端
  路径，状态一致；SQL mirror / index 更新自动走对路径。

## Verification

- `cargo check` ✅
- `cargo test`（含 7 新 /note 测试 + 全表 1092 通过）✅
- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.23s)
