# TG bot `/markers` 命令 — 一次列 pinned + silent 联合

## 背景

iter #205/#212 加了 `/silent` `/unsilent` `/silenced`，与既有 `/pin` `/unpin` `/pinned` 形成两个独立 marker 命令族。owner 想"一眼看到我标过的所有 owner-intent markers" 时要发两条命令（`/pinned` + `/silenced`），各等一段反馈，再脑内合并。

加 `/markers` 一条命令同时列两段。

## 改动

### `src-tauri/src/telegram/commands.rs`

#### 1. 新 TgCommand::Markers 变体（无参）+ name + parse + register + help

```rust
Markers,  // 无参联合查询

// name(): "markers"
// title() union with Pinned / Silenced / Tasks / ... 同
// parse_tg_command: "markers" => Some(TgCommand::Markers)
// get_bot_commands EN/CN
// help text
```

#### 2. 新 `format_markers_list` helper

```rust
pub fn format_markers_list(views: &[TaskView]) -> String {
    let pinned: Vec<&TaskView> = views.iter().filter(|v| v.pinned).collect();
    let silent: Vec<&TaskView> = views.iter().filter(|v| parse_silent(&v.raw_description)).collect();
    if pinned.is_empty() && silent.is_empty() {
        return "暂无 owner-intent markers...用 /pin <标题> 钉住...或 /silent <标题> 让 LLM 不主动选...";
    }
    let mut out = format!("owner-intent markers · 📌 {} 钉 / 🔇 {} 静\n", pinned.len(), silent.len());
    if !pinned.is_empty() {
        out += &format!("\n📌 钉住（{}）\n", pinned.len());
        for v in &pinned {
            out += &format_task_line(emoji_for_status(v.status), v) + "\n";
        }
    }
    if !silent.is_empty() {
        out += &format!("\n🔇 静默（{}）\n", silent.len());
        for v in &silent {
            out += &format_task_line(emoji_for_status(v.status), v) + "\n";
        }
    }
    truncate_if_overflow(out.trim_end_matches('\n').to_string(), pinned.len() + silent.len())
}
```

- 双段分别渲染 —— 同一 task 同时是 pinned + silent 时两段都列（用户视觉 audit 友好）
- 空集合教学引导两条命令 `/pin` + `/silent`
- header 显数量对照
- truncate_if_overflow 用 union 数兜底

#### 3. 3 个新单测

- `parses_markers`：无参 + 大小写不敏感 + 尾部尾巴忽略
- `format_markers_list_empty_teaches_both_commands`：空集合教学含 `/pin` + `/silent`
- `format_markers_list_separates_pinned_and_silent_sections`：含 3 个 task（pin-only / silent-only / both），验证 header counts + 双段渲染 + Both task 在两段各出现一次

### `src-tauri/src/telegram/bot.rs`

handler：read_tg_chat_task_views chat-scoped + filter union（pinned 或 silent）+ format_markers_list 内部再分组渲染。

## 关键设计

- **filter union 而非两个 disjoint set**：handler 把 pinned 或 silent 都收 → format helper 内部分两段渲染。让 Both task 一处过滤即可，避免双重 invoke。
- **同 task 同时 pinned + silent 双段都列**：让 owner 视觉 audit 友好 —— "Pin-only/Silent-only/Both" 三类一眼分。pinned section count 等于 .pinned 的 union；silent section count 等于 [silent] 的 union。
- **format_task_line 复用**：与 /pinned / /silenced 同 emoji 映射 + task line 格式。
- **空集合教学双 command**：onboarding 引 owner 两条 marker 命令都用上。
- **truncate_if_overflow 用 union 数**：通常 owner 标 < 10 个 markers，单消息 4KB 足够 —— 防御性 truncate。

## 不做

- **不引入"我标过但用过太久的 markers" 警告**：marker 是 owner 显式意图，不该 nag.
- **不 list snooze / blockedBy 等其它 markers**：snooze 是时间维度（自然过期），blockedBy 是依赖维度（可视化为依赖图更合适）。本 iter 只覆盖 owner-intent 标记族（pinned + silent）。

## 验证

- `cargo check` ✓
- `cargo test --lib telegram::commands::tests::parses_markers / format_markers_list*` ✓ 3 新单测 passed
- 改动 ~130 行（commands.rs variant + name/title + parse + register + help + format helper 55 + tests 60；bot.rs handler 14）。既有 /pinned / /silenced / /silent / format_pinned_tasks_list / format_silenced_tasks_list 路径完全不动。

## TODO 状态

剩 2 条留池：
- butler_task 行 [reminderMin: N] chip click 弹快速编辑
- pet 区 hover 显本机时区 chip 浮卡

## 后续

- `/markers <kind>` 子参数：`/markers pinned` / `/markers silent` 仅显单段（与 /pinned / /silenced 等价）；保留作 quick aliases。
- 拓展为 `/markers all` 包含 snooze + blockedBy 让 owner 看依赖 / 时间 marker 总览。
- header 顶 sparkline 显 N 个 markers 的"标了多久" age 分布（让 owner 看到自己 marker 积累习惯）。
