# TG bot 加 `/peek <title>` 紧凑视图命令（iter #492）

## Background

`/show <title>` 已显完整 raw description + detail.md 预览（前 300 字符），
适合 "我想看这条 task 到底长啥样" 场景。但当 owner 仅想 "快瞄一眼这
条 status / pinned / silent / 优先级" 时 `/show` 输出过冗 — 含完整
raw text + detail body，占据屏幕。

本 iter 加 `/peek <title>` — **一行紧凑视图**，与 `/show` 完整视图互补：

- `<status_emoji> 「<title>」 · 🕐 <schedule> · <markers> · P<n>`
- 各可选段（schedule / markers / priority）按内容存在与否拼入；空段省略
- 不读 detail.md — 仅 raw_description + status

## Changes

### `src-tauri/src/telegram/commands.rs`

#### Enum 变体（紧贴 Show）

```rust
Peek { title: String },
```

#### 6-surface 同步

按既有 TG command 6-surface drift-defense pattern 全量同步：

1. `name()` arm → `"peek"`
2. `title()` arm → 与 Show / Timeline 同 single-title 共用列
3. parser arm `"peek"` → `TgCommand::Peek { title }`（空 title 留 handler）
4. registry en + zh entries（"One-line compact view: ..." / "一行紧凑
   视图：..."）
5. `ALL_HELP_TOPICS` 加 `"peek"`
6. `format_help_for_topic("peek")` 详细文案（用法 + 输出格式 + 示例 +
   相关）
7. `format_help_text` 表格行（在 `/show` 下一行）
8. 两份 drift-defense test 列表（`format_help_for_each_listed_command`
   + `tg_command_registry_covers_all_user_facing_commands`）

#### 纯 formatter `format_peek_reply`

```rust
pub fn format_peek_reply(
    title: &str,
    raw_description: &str,
    status: TaskStatus,
) -> String
```

实现：

- status → emoji（⏳ / ✅ / ⚠️ / 🚫，与 /show 同表）
- 扫 raw_description 起始 `[every|once|deadline: ...]` 前缀（与
  `parse_butler_schedule_prefix` 同语义但仅展示文本，不解析时刻）→ 🕐
  段
- 扫全 raw 找 `[pinned]` / `[silent]` / `[snooze: ...]` / `[blockedBy:
  ...]` → 📌 / 🔇 / 💤 / 🔒 markers 段
- 扫 `[task pri=N]`（N 单字符 0..=9）→ `P{N}` 段
- 段间 ` · ` 分隔；空段省略 — 极端 raw 完全空 → 仅 `<emoji> 「title」`

**为什么不复用 `extract_marker_tokens`**：那个白名单含 done / error /
result / cancelled / archived（"状态变化" marker），与 /timeline 用
途匹配。/peek 的 status 已用 emoji 表达，重复显冗余；想要的是 owner
活跃干预 markers（pinned / silent / snooze / blockedBy）— 不同 axis。

### `src-tauri/src/telegram/bot.rs`

#### Handler

```rust
TgCommand::Peek { title } => {
    if title.trim().is_empty() {
        format_missing_argument("peek")
    } else {
        let actual = ...;  // 三层 resolve 同 /show
        match actual {
            Ok(t) => {
                let views = read_tg_chat_task_views(chat_id.0);
                match views.iter().find(|v| v.title == t) {
                    Some(v) => format_peek_reply(&v.title, &v.raw_description, v.status),
                    None => ...
                }
            }
            Err(msg) => format_command_error(&msg),
        }
    }
}
```

不调 `task_get_detail`（省一次 detail.md IO）— 紧凑视图只需 view 已含
的 raw_description + status。

## Key design decisions

- **不读 detail.md**：紧凑视图本质就是 "快瞄"，detail.md 内容是 /show
  的领域。IO 越少响应越快，TG 端体验改善
- **三层 resolve 与 /show / /done / /cancel 同源**：数字 index → fuzzy
  → 错误候选 — 一致 UX，owner 肌肉记忆复用
- **不显 [done] / [error] / [result] markers**：状态本身已在 emoji 段
  表达（⏳/✅/⚠️/🚫），重复显是冗余 — /timeline 是看历史演化的，那里
  状态变化才有意义
- **`[task pri=N]` 单字符**：与 parse_task_prefix 同源约定 — pri 设计
  上只 0..=9，多字符场景视作 malformed 不显
- **schedule 段仅认起始前缀**：与 `parse_butler_schedule_prefix` 同语
  义，中段出现 `[every: ...]` 不算 schedule 是设计约定，避免 markers
  body 里 mention "[every]" 误识别
- **段间 ` · ` 分隔**：与既有 /now / /streak / /aware 等 status-line
  命令一致格式
- **markers 段 emoji 顺序固定**（📌 🔇 💤 🔒）：稳定 visual scan，不
  按 raw 内出现顺序排（那个易随 markers 重排扰动）
- **14 个 unit tests pin 真实行为**（满足 GOAL.md "meaningful tests
  only" — 每条测一种语义路径：emoji map / schedule prefix detect /
  mid-text reject / markers detect / status-change exclude / priority
  parse / 全组合 layout）

## Verification

- `cargo build`（src-tauri）— clean (warnings only — 既有 dead_code 提
  示，无新错误)
- `cargo test --lib`（全部 1585 个 test）— all pass
- 新加 14 个 peek tests — all pass
- 三个 drift-defense test（registry / help-detail / name+title）— all
  pass

## Future iters (out of scope)

- `/peek_all` / `/peek_pinned` 批量紧凑视图（一屏看多条 status snap）
- schedule 段相对时间增强：「每天 09:00 · 还 3 小时」— 需引时钟计算，
  当前刻意保 "schedule 段纯文本透显" 简洁
