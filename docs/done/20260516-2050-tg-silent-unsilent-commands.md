# TG bot `/silent` + `/unsilent` 命令

## 背景

silent marker 命令族跨端覆盖最后一块 —— 桌面已通过 `[silent]` 字面量 (iter #193) + 右键菜单一键 toggle (iter #199) 覆盖；PanelMemory 显 silent count chip + 可点 filter (iter #197/202)。TG bot 端目前仍要 owner 通过桌面 panel 标 silent —— 不连贯。

本 iter 加 `/silent <title>` + `/unsilent <title>` 完成跨端命令族（与 `/pin` / `/unpin` 完全对偶）。

## 改动

### `src-tauri/src/telegram/commands.rs`

#### 1. 新 TgCommand 变体

```rust
/// `/silent <title>` —— 给任务加 `[silent]` marker。
Silent { title: String },
/// `/unsilent <title>` —— 清掉 `[silent]` marker。
Unsilent { title: String },
```

#### 2. `name()` / `title()` 方法扩

```rust
TgCommand::Silent { .. } => "silent",
TgCommand::Unsilent { .. } => "unsilent",
// title() 内 union pattern 加两条
```

#### 3. `parse_tg_command` 加 dispatch

```rust
"silent" => Some(TgCommand::Silent { title }),
"unsilent" => Some(TgCommand::Unsilent { title }),
```

#### 4. `get_bot_commands` 注册（中英）

```rust
("silent", "Mark a task as [silent] (LLM won't auto-pick; manual fire still works)"),
("unsilent", "Clear a task's [silent] mark"),
// CN: ("silent", "标静默（LLM 不主动选；面板 / 手动触发不受影响）"), ...
```

让 TG bot autocomplete 显示新命令。

#### 5. help text 扩

```
/silent <title> | /unsilent <title>  —  标静默 / 解除静默（LLM 不主动选；面板仍可手动触发）
```

#### 6. 2 个新单测

- `parses_silent_unsilent`：多 token title + 大小写不敏感
- `parses_silent_unsilent_empty_title`：空 title 由 handler 走 missing-argument 路径

### `src-tauri/src/telegram/bot.rs`

#### 1. missing-argument gate 加 Silent / Unsilent

```rust
TgCommand::Silent { ref title }
| TgCommand::Unsilent { ref title }
    if title.trim().is_empty() => format_missing_argument(cmd.name()),
```

#### 2. 命令 handler 加 Silent / Unsilent branch

与 Pin / Unpin 同模板：三层 resolve（数字编号 / fuzzy / exact）+ 调 `task_set_silent` Tauri 命令（iter #199 加的 backend）+ 反向命令提示。

```rust
TgCommand::Silent { title } => {
    let actual = ... three-layer resolve ...
    match actual {
        Ok(t) => match crate::commands::task::task_set_silent(t.clone(), true) {
            Ok(()) => format!("🔇 已标 silent「{}」\nLLM 不再主动选；如需恢复发 /unsilent {}", t, t),
            Err(e) => format_command_error(&e),
        },
        Err(msg) => format_command_error(&msg),
    }
}
TgCommand::Unsilent { title } => {
    ... 同上但 silent: false + label "已解除 silent" ...
}
```

## 关键设计

- **完全镜像 /pin /unpin pattern**：variant / name / title / parse / register / help / handler / tests —— 7 个 surface 全镜像。 silent 与 pin 是同维度（owner 意图 marker），UX / 实现层一致。
- **三层 resolve 复用既有 helper**：try_resolve_by_index → resolve_tg_task_title。owner 可以 `/silent 3` 用数字、`/silent 整理` fuzzy、或完整 title。与 done / pin / snooze 同。
- **反向命令提示**："已标 silent 如需恢复发 /unsilent X"。让 owner 不需翻 help 文档就能立刻撤销。
- **strip-before-write 幂等**：task_set_silent backend 已经在 iter #199 实现 atomic strip + append；这里直接复用，silent state 不会因多次 /silent 命令累积 marker。
- **不写新 list 命令 `/silenced`**：silent 任务通常不该被列在 owner 视野（owner 标 silent 就是为了"暂时不想看"）。需要回看时桌面 PanelMemory 已有 🔇 N silent chip click filter（iter #202）。
- **不在 /tasks 列表中区分 silent**：silent 任务仍属于本会话派单，应显在 /tasks 中。与桌面 PanelTasks queue 同 —— silent 主要影响 LLM proactive cycle，不影响 owner 自己的视野。

## 不做

- **不加 `/silenced` 列 silent 任务清单**：用例稀疏；桌面已有 chip click filter。
- **不写 frontend 改动**：TG bot 只影响 TG 端 UX；桌面已经覆盖完整 surface。
- **不让 /silent + /pinned 互斥**：owner 可同时标 silent + pinned 表"重要但不希望 pet 主动催"（即"我会自己做但请记下来"）。两个 marker 维度独立。

## 验证

- `cargo check` ✓
- `cargo test --lib telegram::commands` ✓ 165 passed（2 新 silent test）
- 改动 ~80 行（commands.rs 50：variant + name/title + parse + register + help + tests；bot.rs 30：missing-argument gate + 2 handler branches）。既有 /pin / /unpin / /snooze / /unsnooze / handler 路径 / set_tg_commands 注册流 / fuzzy resolve / 数字 resolve 全部不动。

## TODO 状态

剩 5 条留池：
- butler_task edit-schedule modal 扩支 every_weekdays
- detail.md 编辑器字数 chip 选区感知
- PanelChat session bar item hover 1s 浮 "最近 3 条" preview
- 任务行 hover preview tooltip 加 "右键查看所有操作" hint
- ChatMini bubble click + ⌘ 复制单条

## 后续

- TG bot 也开 `/silenced` 列 silent 任务清单（懒癌救星 owner 想"我标过哪些 silent"），就像 /pinned 一样。
- LLM butler_task_edit tool 接受 silent: bool 字段（当前 LLM 不该自由 toggle owner 意图；可以只允许 LLM 写 silent=true 不允许 false——"LLM 自己识趣地静默" 单向许可）。
