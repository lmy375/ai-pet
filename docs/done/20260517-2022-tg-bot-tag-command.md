# TG bot `/tag <name>` 命令（iter #385）

## Background

桌面 PanelTasks 已有 #tag chip click filter — 点击 chip 把视图收
窄到含该 tag 的 task。手机端 owner 想"按 tag 筛 task" 没入口 —
要走 /tasks 看全表自己肉眼挑。

本 iter 加 TG `/tag <name>` 命令 — 与桌面 #tag chip filter 对偶
audit。与既有 `/tags`（列所有 tag 名 + 各任务数 top 15）正交 — tags
是 tag 名清单 audit，本命令是按某 tag 列 task 清单 audit。

## Changes

### `src-tauri/src/telegram/commands.rs`

#### 1. enum 变体（~line 221）

```rust
Tag { name: String },
```

#### 2. `name()` → "tag"；`title()` arm 加入 text-bearing 共用簇

#### 3. parser（~line 873）

`#` 前缀 strip + 取首 token：
```rust
"tag" => {
    let raw = title.trim().trim_start_matches('#').trim();
    let name = raw.split_whitespace().next().unwrap_or("").to_string();
    Some(TgCommand::Tag { name })
}
```

接受 `/tag 工作` / `/tag #urgent` / `/tag #` (空 name)。多余尾部一
律 split_whitespace 取首即丢（与 parse_task_tags 不含空格的 tag 边
界一致）。

#### 4. `format_tag_reply(views, name)` pure formatter（~line 2308）

入参 chat-scoped views + name（来自 parser，前缀已 strip）：
- 空 name → usage hint（含 /tags 交叉引用）
- 无命中 → "没有任务带 #<name>" + 推 /tags
- 有命中 → status emoji + title + 紧凑 due（MM-DD HH:MM 切片）

匹配语义：**case-insensitive exact 等值**（`v.tags.iter().any(|t|
t.to_lowercase() == kw_lower)`）— 与 /find 子串搜正交。help text
明示二者差异。

排序：status_rank pending(0) < error(1) < done(2) < cancelled(3)
（与 format_find_reply 同节奏 — owner 优先 audit 活跃任务）。同
status 保 views 原序。

cap 20 + overflow hint（与 /tasks / /find 同 N=10/20 阶梯）。

#### 5. registry zh + en + format_help_text + format_help_for_topic
+ ALL_HELP_TOPICS + 两 drift-defense 列均加 "tag"

### `src-tauri/src/telegram/bot.rs`

新 handler arm（紧贴 Find 之后）：

```rust
TgCommand::Tag { name } => {
    let views = read_tg_chat_task_views(chat_id.0);
    format_tag_reply(&views, &name)
}
```

复用 chat-scoped read path。

### Tests（commands.rs，10 个新 unit test）

Parser（4 个）：
- bare name → exact
- `#` 前缀 strip → exact name
- 多余尾部 token 丢
- 空 name / 仅 `#` → 空

Formatter（6 个）：
- 空 name → usage hint + /tags 交叉引用
- 无命中 → bootstrap + /tags 推荐
- 多命中 → status emoji + 仅命中 task
- case-insensitive 匹配（URGENT match urgent）
- pending 排 done 之前
- due 含时显 MM-DD HH:MM 紧凑
- 25 条 → overflow "还有 5 条"

## Key design decisions

- **exact 等值而非子串**：与 /find（子串 title + description）正
  交。/tag 「健身」要找的是 `#健身`，不是 `#健身房` 那种 prefix
  误命中。tag 是结构化标签，应精确匹配。
- **`#` 前缀 strip 而非要求显式**：owner 心智里 chip 是 `#健身`
  形态，命令输入也写 `#健身` 自然 — 兼容也接受 bare `健身`。
- **caller 在 parser strip 而非 formatter**：parser 把 raw input
  正规化（含 `#` strip + 首 token 取），formatter 收已规整 name
  专注呈现。一致与 /pri / /promote / /demote 等 normalize 模式。
- **首 token 取而非 join 全段**：parse_task_tags 边界 tag 不含空
  格 — 多余尾部不可能是合法 tag。"/tag 工作 demo" 取 工作 不算错。
- **cap 20 比 /find 的 10 更高**：tag 是显式归类，单 tag 下 task
  数通常较多（owner 给 task 加 tag 时已自己做了一层筛）。20 给
  /tasks /pinned 同级容量。
- **不为单 fn 引专项 setup**：复用既有 `view` + `view_with_tags`
  helper。test 数据覆盖 5 个核心场景就够。

## Verification

- `cargo check`（backend）— clean
- `cargo test --lib`（backend）— **1356 passed / 0 failed**（+11
  新 tag test，两 drift-defense 列也命中 "tag"）
- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.28s)
