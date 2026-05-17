# TG bot `/tags` 命令（iter #313）

## Background

owner 在 TG 端有 `/markers` 看系统 marker（pinned + silent）矩阵，但缺
owner 自定义 `#tag` 维度的 audit 入口。task description 里随手写的
#健身 / #读书 / #工作 等自定义 tag 当前只能通过 `/find #健身` 命令一个个
搜，无法一眼看 "我用过哪些 tag 矩阵长什么样"。

本迭代加 `/tags` 列本 chat 派单中所有用过的 `#tag` + 各 tag 任务数。

## Changes

### `src-tauri/src/telegram/commands.rs`

- enum 加 `Tags` 无参变体（与 `Markers` / `Pinned` / `Silenced` 同模板）
- `name()` → "tags"；`title()` 归入无参桶
- 解析器："tags" 分支无参；多余尾部一律忽略（与 /markers 容忍一致）
- 新常量 `TAGS_CAP = 15`
- 新 pure formatter `format_tags_reply(views)`：
  - 用 `BTreeMap<String, u32>` 聚合 — BTreeMap 自然字典序为 ties 提供
    stable secondary sort
  - 跳过 trim 后空 tag（防御性）
  - 按 count desc 排序（stable，ties 走字典序）
  - 空矩阵 → 友好兜底文案 + untagged 数 + 怎么用 `#name`
  - 非空：头部 `🏷 /tags（共 N 个 tag）` + 列前 15 + "其它 N 个" overflow
    + "无 #tag 任务：M 条" 让 owner 看分母
- registry zh + en 都加 ("tags", desc)
- format_help_text 全表加 `/tags` 行（/markers 之后）
- format_help_for_topic 加 "tags" key + 与 /markers / /find 交叉引用
- 两 drift-defense 名单（format_help_for_each_listed_command +
  tg_command_registry_covers）都加 "tags"

### `src-tauri/src/telegram/bot.rs`

- 加 `TgCommand::Tags` handler arm（在 Markers arm 之后）：
  read_tg_chat_task_views + format_tags_reply 一次性

### Tests（7 个新 unit test）

- parser：无参 / 多余尾部容忍
- formatter：
  - 空 views → 兜底文案 + untagged 0
  - 排序 by count desc（健身 3 / 读书 2 / 晨练 1 矩阵验证）
  - 排除 untagged 走单独行
  - cap 15 + "…还有 N 个 tag" overflow（生成 20 tag 验证）
  - 跳过 trim 后空 tag 字符串
  - 跨 status 计数（done / cancelled 也算 — audit 维度）

## Key design decisions

- **跨 status 计数而非仅 active**：`/tags` 是"audit 维度"（我用过哪些 tag），
  与 /tasks（看清单）/ /find（搜任务）不同。owner 创建 "#健身" 任务后
  完成它，下次看 /tags 应仍能看到 — 这与 /digest 看 "最近 done" 同理：
  历史数据是有效信号。
- **`BTreeMap` 字典序 + sort_by(count desc) 二阶**：让 ties 行为确定可
  测（"两个 tag count 相同时按字母序"）。HashMap 顺序不确定会让 test
  flaky。
- **cap 15 而非更多**：TG message 4096 字符限制 + owner 实际不会需要看
  超 15 个 tag 一屏（top 15 已覆盖 80/20 信号）。超出汇总"…还有 N 个"
  保留 hint 但不展开。
- **untagged 数单独显**：让 owner 看到分母 — "20 条任务里 15 条有 tag /
  5 条没 tag" 比单纯 "tag 矩阵" 信号更完整。空矩阵时仍显 "K 条任务无
  tag" 防误以为 "我连 task 都没有"。
- **教育文案 in 空兜底**：空矩阵 reply 含 `创建任务时在 description 写
  '#name' 即被自动收录` —— 新 owner 第一次 /tags 没数据时直接学会怎么
  生成 tag。

## Verification

- `cargo test --lib`（backend）— 1160 passed / 0 failed（7 新 tags 测试
  通过；drift-defense 也命中新加的 "tags"）
- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.22s)
