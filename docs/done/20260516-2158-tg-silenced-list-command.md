# TG bot `/silenced` 命令 — 列本 chat 所有 silent 任务

## 背景

iter #205 加了 `/silent` + `/unsilent` 命令完成跨端 silent 标记。但 owner 想 "audit 我标过哪些 silent" 时仍要回桌面 PanelMemory 看 silent count chip 然后 click filter（iter #197/202）。TG 端缺一条对应的 `/silenced` 列表命令。

与 `/pinned` 完全对偶。

## 改动

### `src-tauri/src/telegram/commands.rs`

#### 1. 新 TgCommand 变体 + name + parse + register + help

```rust
Silenced,  // no args

// name(): "silenced"
// title(): "" (无参 enum 与 Tasks/Pinned/Mood/... 同 union)

// parse_tg_command: "silenced" => Some(TgCommand::Silenced)
// get_bot_commands EN/CN: ("silenced", "List currently silent tasks ...") / ("silenced", "列出本聊天派单中所有 silent 任务...")
// help text: "/silenced  —  列出本聊天派单中所有 silent 任务（按状态分组）"
```

#### 2. 新 `format_silenced_tasks_list` helper

```rust
pub fn format_silenced_tasks_list(views: &[TaskView]) -> String {
    if views.is_empty() {
        return "🔇 暂无静默任务...用 /silent <标题> 标静默，或在桌面任务面板右键 → 「🔇 标 silent」。";
    }
    let mut pending/done/error/cancelled = Vec::new();
    for v in views { match v.status { ... } }
    let mut out = "🔇 当前静默任务（共 N 条 · LLM 不主动选）\n";
    sections: [("进行中","⏳"), ("已完成","✅"), ("已失败","⚠️"), ("已取消","🚫")];
    for (label, emoji, items) in &sections {
        if items.is_empty() continue;
        out += &format!("{}（{}）\n", label, items.len());
        for v in items { out += &format_task_line(emoji, v) + "\n"; }
    }
    truncate_if_overflow(trim, views.len())
}
```

完全镜像 `format_pinned_tasks_list` — 状态分组（pending / done / error / cancelled）+ emoji 标记 + truncate_if_overflow 兜底过长。

#### 3. 3 个新单测

- `parses_silenced`：无参 + 大小写不敏感 + 尾部尾巴忽略
- `format_silenced_tasks_list_empty_teaches_silent_command`：空集合教学含 🔇 + `/silent` + 桌面入口
- `format_silenced_tasks_list_sections_show_per_status`：含 1 条 pending 任务时 header "共 1 条" + section "进行中" + 🔇 emoji

### `src-tauri/src/telegram/bot.rs`

#### 4. handler

```rust
TgCommand::Silenced => {
    let views = read_tg_chat_task_views(chat_id.0)
        .into_iter()
        .filter(|v| crate::task_queue::parse_silent(&v.raw_description))
        .collect();
    format_silenced_tasks_list(&views)
}
```

- read_tg_chat_task_views：与 /pinned / /tasks 同 read path 已 chat-scope 过滤
- parse_silent：iter #199 引入的 backend helper 严格字面 `[silent]` 检查

## 关键设计

- **完全镜像 /pinned**：variant / name / parse / register / help / format / handler 全镜像。silent 与 pinned 是相同维度（owner 意图 marker），UX 一致。
- **空集合教学**：与 /pinned 同模板 —— 头 + 教 `/silent` 语法 + 桌面入口提示。
- **状态分组**：与 /pinned 同 4 段（pending/done/error/cancelled）。silent 任务跨所有状态可存在（marker 是 owner 偏好与状态正交）。
- **truncate_if_overflow 复用**：TG 单消息 4KB 上限。多 silent 任务时 fallback 截断 + "（还有 N 条…）"。
- **不缓存**：用户连发 /silenced 就是想"看现在"。

## 不做

- **不在 /tasks 总列表区分 silent**：silent 任务仍属本会话派单。owner 看 /tasks 时若想区分 silent，可走 /silenced 单查。
- **不让 /silenced 隐 done/cancelled** silent：silent 与 status 正交 —— done 任务也可标 silent（owner 留作"备查不再主动催"）。section 渲染让 owner 自行扫读。
- **不写 frontend 改动**：TG bot 仅影响 TG 端 UX；桌面 PanelMemory section header 🔇 N silent chip + iter #202 filter 已覆盖。

## 验证

- `cargo check` ✓
- `cargo test --lib telegram::commands::tests::parses_silenced / format_silenced_*` ✓ 3 新单测 passed
- 改动 ~120 行（commands.rs variant + name/title + parse + register + help + format helper 50 + tests 50；bot.rs handler 12）。既有 /pinned / /silent / /unsilent / format_pinned_tasks_list / read_tg_chat_task_views 路径完全不动。

## TODO 状态

剩 4 条留池：
- PanelTasks "新建任务" + ⇧Enter 创建并立即打开 detail 编辑器
- butler_task_edit LLM 工具 description 加 marker 教学
- PanelMemory ai_insights 类目顶 "🧠 由宠物自己写" banner
- 桌面 pet 右键加「⏰ 设倒计时 N 分钟 nudge」

## 后续

- TG /pinned + /silenced 联合命令 `/markers` 一次列两类（让 owner 一眼看全 owner-intent markers）。
- /silenced 内带 ⏰ 时间 ts 信息 "标 silent 于 N 天前"，让 owner 看"标得太久了该 review 一遍" 信号。
- TG /unsilent 多 title 批量 unmute（"我标过的 silent 都解除一下"）。
