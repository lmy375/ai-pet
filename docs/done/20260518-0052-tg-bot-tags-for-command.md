# TG bot `/tags_for <title>` 命令（iter #434）

## Background

`/tags` 给 owner 全聊天 #tag 矩阵 (top 15 tag + 各 task 数)；
`/tag <name>` 反向（含该 tag 的所有 task）。但**单条 task 的 tag
清单**没专用入口 — owner 想「这条 task 标了哪些 tag」必须
`/show <title>` 看 raw_description 含 `#tag` tokens + 心算解析。

本 iter 加 `/tags_for <title>` 单条聚焦视图 — 直接读
`TaskView.tags` Vec 列出。

## Changes

### `src-tauri/src/telegram/commands.rs`

#### 1. `TgCommand::TagsFor { title }` 变体

紧贴 `Tag`（tag 视图族）。snake_case `tags_for` 避开 dash drift-defense。

#### 2. 解析

```rust
"tags_for" => Some(TgCommand::TagsFor { title }),
```

与 /show / /timeline / /forks 同 single-title pattern。

#### 3. `format_tags_for_reply(views, target_title)` pure 函数

```rust
let target_view = views.iter().find(|v| v.title == target)?;
if target_view.tags.is_empty() {
  return "🏷 「<title>」无 #tag 标记。\n在 description 写 `#name`...";
}
let tags_str = target_view.tags.iter()
  .map(|t| format!("#{}", t.trim())).collect::<Vec<_>>().join(" ");
format!("🏷 「{}」{} 个 tag：\n{}", target, target_view.tags.len(), tags_str)
```

4 态状态机：
- 空 title → usage hint
- target 在 views 找不到 → "没找到 task" 错误
- target.tags 空 → "无 #tag 标记" + 教学语法
- 有 tags → 「🏷 N 个 tag：#a #b ...」

#### 4. Registry + ALL_HELP_TOPICS + help-for-topic + table line + drift defense

- 双 lang registry 各加（紧贴 tag）
- ALL_HELP_TOPICS 紧贴 "tag"
- format_help_for_topic 长详细文案（含与 /tags / /tag / /show
  对比）
- /tag help 末追加交叉引用 /tags_for
- format_help_text 全表加一行
- 两处 drift-defense 测试列表加 "tags_for"

### `src-tauri/src/telegram/bot.rs`

handler 紧贴 `Tag`：

```rust
TgCommand::TagsFor { title } => {
    if title.trim().is_empty() {
        format_missing_argument("tags_for")
    } else {
        let actual = match try_resolve_by_index(...).await {
            Some(t) => Ok(t),
            None => resolve_tg_task_title(&title),
        };
        match actual {
            Ok(t) => {
                let views = read_tg_chat_task_views(chat_id.0);
                format_tags_for_reply(&views, &t)
            }
            Err(msg) => format_command_error(&msg),
        }
    }
}
```

reuse 既有 3 层 title resolve + chat-scoped views pipeline。

### 6 单元测试

parse（含 title / 空 title）+ formatter 4 个状态（empty target /
target not found / no tags / lists with count）。

## Key design decisions

- **直接读 TaskView.tags Vec**：tag 提取已在 `build_task_view` 内
  解析好（`parse_task_tags` 扫 `#name` tokens），formatter 不重做
  解析
- **空 tags 教 syntax**：与 /tags 全空时同教学 pattern — 让 owner
  知道如何标 tag 才有内容
- **3 层 title resolve**：与 /show / /forks / /timeline 同 muscle
  memory；数字 index / fuzzy / 错误候选
- **不为 tag 加排序 / 计数**：单 task 标 < 10 tag 是常态，原顺序
  显即可；count 字段已在 header
- **6 测试**：parse 2 个 + formatter 4 个 — 覆盖 happy path + 3 个
  edge case（空 / 不存在 / 无 tag）

## Verification

- `cargo test --lib telegram::commands::tests::tags_for` — 6 / 6 通过
- `cargo test --lib`（全表）— 1478 / 1478 通过
- `npx tsc --noEmit`（frontend）— clean（无变更）
- `npx vite build`（frontend）— clean (1.29s)
