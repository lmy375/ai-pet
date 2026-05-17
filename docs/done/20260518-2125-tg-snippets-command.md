# TG bot 加 `/snippets` 命令（iter #502）

## Background

owner 常把可复用片段（prompt 模板 / 决策清单 / 常用回复 / 流程
checklist）写成 task — 但没有统一 marker 让 owner 一眼看「我都标了
哪些可复用」。要找时只能靠 #tag（owner 自打）或 fuzzy /find，缺中央
集中入口。

本 iter 引一个 `[snippet]` / `[snippet: <label>]` marker 约定 + 配套
`/snippets` 命令一次性列出。owner 用 `/edit <title> :: [snippet:
PR template] body...` 标记，之后 `/snippets` 集中 audit；想用某条走
`/show` 看完整；想克隆改装走 `/dup`。

## Convention

marker 格式三种全支持：

- `[snippet]` — 无 label，简单标"可复用"
- `[snippet: <label>]` — 含 label 区分用途（如 "PR template" / "决策开头"）
- `[snippet：<label>]` — 全角冒号（中文键盘半角忘切场景）
- `[snippet <label>]` — 空格分隔（不常见但兼容）

边界防御：
- `[snippetXY]` 不命中（防 token-boundary 碰撞）
- 多次出现仅取首个

## Changes

### `src-tauri/src/telegram/commands.rs`

#### Enum + 6-surface sync

按既有 TG command 模式同步：

1. **Enum 变体** `Snippets`（无参，紧贴 Dup 之后）
2. **`name()` arm** → `"snippets"`
3. **`title()` arm** → 加入无参 arm 集（Tasks / Pinned / Silenced /
   Markers / Tags / ...）
4. **parser arm** `"snippets" => Some(TgCommand::Snippets)`
5. **en + zh registry** entries
6. **`ALL_HELP_TOPICS`** 加 `"snippets"`
7. **`format_help_for_topic("snippets")`** 详细文案（含 marker 约定 +
   输出格式 + 教学例 + 与 /dup / /show / /markers 互补关系）
8. **`format_help_text`** 表格行（在 /dup 下一行）
9. **两份 drift-defense test 列表**

#### 新 pub helper `parse_snippet_marker`

```rust
pub fn parse_snippet_marker(description: &str) -> Option<String> {
    // 扫 `[...]` 段，inner 以 "snippet" 起 + 后接 ` ` / `:` / `：`
    // / `]` 才算命中（防 [snippetXY] 碰撞）
    // 命中后返回 label trim 后字符串：`[snippet]` → Some("")；
    // `[snippet: PR template]` → Some("PR template")
    ...
}
```

5 个 unit tests 覆盖路径：absent / empty label / labeled / 多种分隔
符（半角:/全角：/空格）/ 多次出现取首个 + token-boundary 防碰撞。

#### 纯 formatter `format_snippets_reply`

```rust
pub fn format_snippets_reply(views: &[TaskView]) -> String {
    if views.is_empty() {
        return "📎 ... 教学兜底文案 + /edit 例子".to_string();
    }
    // 渲染：📎 snippets · N 条 + 每行 status_emoji + title +
    // [label]（非空时显）+ body 前 80 字预览（split_whitespace flatten）
    ...
}
```

3 个 unit tests：空集教学兜底 / 含 label vs 不含 label 双格式 / 长
body 截断 + …。

### `src-tauri/src/telegram/bot.rs`

Handler 紧贴 Dup 之后（标 Snippets 是无参命令）：

```rust
TgCommand::Snippets => {
    let views: Vec<TaskView> = read_tg_chat_task_views(chat_id.0)
        .into_iter()
        .filter(|v| {
            crate::telegram::commands::parse_snippet_marker(
                &v.raw_description,
            ).is_some()
        })
        .collect();
    crate::telegram::commands::format_snippets_reply(&views)
}
```

与 /pinned / /silenced 同 chat-scope filter 模板。

## Key design decisions

- **marker-based 而非独立 category**：snippet 是 cross-cutting 属性
  （工作 snippet / 个人 snippet 跨多 category），用 marker 标比新建
  category 灵活
- **复用 `/dup`**：`/dup <snippet title>` 自然保留 `[snippet]` marker
  → 副本仍是 snippet，可继续做模板基底（与 schedule / pinned 等 owner-
  intent markers 一起继承）
- **label 可空**：`[snippet]` bare marker 也算合法；不强求 label 让
  入门最低门槛（owner 想分门别类时再加 label）
- **全角冒号支持**：中文键盘常见困扰；与 `[snooze: 09:00]` / `[every:
  09:00]` 等既有 parser 全角宽容传统一致
- **body 预览 80 字 + 单空格 flatten**：与既有 /find 命中 snippet 同
  字数；flatten 让多行 task 在一行显
- **空集 friendly 教学**：N === 0 时不光说"无"，给 /edit 示例教如何标 —
  本命令双重作用「audit + 推广 marker 约定」
- **不引前端入口**：marker 写入由 owner 通过 /edit 完成，本 iter 仅加
  TG 端列表入口。前端 PanelTasks「✏️ rename 后」加 inline snippet
  toggle chip 可作未来 iter
- **8 个 unit tests pin 真实行为**（满足 GOAL.md "meaningful tests
  only"）：parser 1 / parse_snippet_marker 4 / format 3。每条覆盖一
  种独立语义路径

## Verification

- `cargo build`（src-tauri）— clean（仅既有 dead_code warnings）
- `cargo test --lib` — all 1603 tests pass（新 8 个 + 既有 1595）
- 三个 drift-defense test all pass
- 手测路径：
  - TG: `/edit 模板A :: [snippet: PR template] 1. diff 2. test 3. comment`
  - TG: `/snippets` → 看到 `📎 snippets · 1 条:` + 「模板A [PR
    template]」+ body 预览
  - TG: `/snippets` 当无标记 task → 友好教学兜底 + /edit 示例

## Future iters (out of scope)

- 前端 PanelTasks 行内「📎 snippet」toggle chip — 鼠标党友好
- `/dup_snippet <label>` — 按 label 直接克隆，免先 /snippets 找 title
- `/edit_snippet <label> :: <body>` — 按 label 直接覆写
- consolidate sweep 时 [snippet] 标过的 task 优先级降权（防 LLM 把模
  板误判成"未完成 task"做合并）
- /markers 矩阵未来扩 snippets 第 3 段（与 pinned + silent 同视图）
