# TG `/pinned` 命令：列钉住任务

## 背景

TODO 上 auto-proposed 一条："TG `/pinned` 命令：列出当前所有 pinned session（与桌面「📌 钉住」chip 同源），让手机端也能快速跳到。"

实施时把 scope 从 session 改成 **task** —— TG bot 是单设备 chat 通道，"列 pinned sessions" 在 TG 端无 actionable 价值（不能 navigate）；但 "列 pinned tasks" 与 TG 既有 `/tasks` / `/done` / `/cancel` / `/snooze` 等任务命令体系完整对偶 —— owner 在 TG 上看到 pinned 列表后能直接 `/done <title>` `/unpin <title>` 操作。pin 是"重要任务"标记，TG 端"我现在关键事情还有哪些"是高频问题。

桌面任务面板已有「📌 N」chip 过滤；TG 端补 `/pinned` 命令让两端体验对称。

## 改动

### `src-tauri/src/telegram/commands.rs`

#### enum 扩展

```rust
/// `/pinned` —— 列出本 chat 派单中所有当前 pinned 任务（与桌面任务面板
/// 「📌 N」chip 同源信号）。无参；多余尾部一律忽略。filter 范围与 `/tasks`
/// 一致（origin == Tg(chat_id)）。
Pinned,
```

`name()` / `title()` / 注册表（zh + en）三处分支补齐。

#### parser

```rust
"pinned" => Some(TgCommand::Pinned),
```

无参；多余尾部一律忽略（与 /tasks 同容忍策略）。

#### `format_pinned_tasks_list` pure helper

```rust
pub fn format_pinned_tasks_list(views: &[TaskView]) -> String {
    if views.is_empty() {
        return "📌 暂无钉住任务（本聊天派单中）。\n用 /pin <标题> 钉住，或在桌面任务面板右键 → 「📌 钉住」.";
    }
    // 与 format_tasks_list 同 section 分组（进行中 / 已完成 / 已失败 / 已取消）
    // 但 header 换 "📌 当前钉住任务（共 N 条）"。
}
```

不复用 `format_tasks_list` —— header 文案 + 空集合教学不同。共享思路但独立函数让两个命令的输出分别可控。

#### `format_help_text` 加新行

```text
/pinned  —  列出本聊天派单中所有钉住任务（按状态分组，含 done/error/cancelled）
```

### `src-tauri/src/telegram/bot.rs`

```rust
TgCommand::Pinned => {
    let views: Vec<TaskView> = read_tg_chat_task_views(chat_id.0)
        .into_iter()
        .filter(|v| v.pinned)
        .collect();
    crate::telegram::commands::format_pinned_tasks_list(&views)
}
```

复用 `read_tg_chat_task_views` 读 path（与 /tasks / /stats 同源 chat-filter）+ 链式 filter pinned。不去重 / 不缓存（与 /stats 同：每次发都是想"看现在到底什么样"）。

### 新 3 单测

1. **`parses_pinned`**：`/pinned`、`/PINNED`、`/pinned now?` 三种形态都映射 `Some(TgCommand::Pinned)`，覆盖大小写不敏感 + 多余尾部容忍。
2. **`format_pinned_tasks_list_empty_teaches_pin_command`**：空集合输出含 `📌` emoji + `/pin` 语法 + "桌面 / 右键" 教学引导 —— 让 owner 看到"暂无钉住"时不光知道这条事实，还知道下一步该怎么做。
3. **`format_pinned_tasks_list_groups_by_status_and_counts`**：3 条 pinned 任务（pending / done / cancelled）→ header 显 "共 3 条" + 各 section 报 "(1)" + 每个 title 出现一次。pin 与 status 正交（done / cancelled 也可标），section 分组验证这种正交关系。

### `tg_command_registry_covers_all_user_facing_commands`

注册表覆盖测试名单加 "pinned"，防 silent drop。

## 关键设计

- **从 session 换 task**：见背景段。原 TODO 没考虑 TG 端 session 不可 navigate 的 architectural reality；task 是真正 actionable 的 pin 维度。
- **chat-filter 范围与 /tasks 对齐**：所有 TG 任务查询命令（/tasks / /stats / /today / /pinned）都按 `origin == Tg(chat_id)` 过滤 —— 让"我从 TG 派的活"成为一致的 view。owner 想看跨 origin 全集需切桌面。
- **空集合教学引导**：`/pinned` 用户第一次输入很可能没钉过任何任务；显"暂无钉住"+ "用 /pin <标题> 钉住" 让命令本身教用户怎么用，省一遍 `/help`。
- **section 分组保留终态**：done / cancelled 任务的 pinned 也保留显示 —— owner 可能想看"上周我标关键的事都做完了吗"。视觉上 pending 仍在最顶，与 `/tasks` 一致。
- **`format_pinned_tasks_list` 独立函数而非 `format_tasks_list` 重用 + 参数化**：两个命令的 header / empty case 差异够大，参数化反而把 format_tasks_list 拉胖。30 行重复 < 80 行参数族（DRY 之外还有"读起来直接"价值）。
- **3 个单测**：parser / empty case / non-empty grouping —— 与既有 format_tasks_list / format_stats_reply 同测试覆盖深度。GOAL.md "禁装饰测试" 原则下，每条都 pin 真实行为：parses_pinned 测大小写 + 容忍尾部、empty 测教学文案、grouping 测 section + count。

## 不做

- **不做 /pinned 的全局（跨 chat）版本**：与 /tasks 同 chat-filter 边界 —— 跨 origin 由桌面承担。多了"我从 TG 看到的列表为啥和桌面不一样" 的认知摩擦。
- **不做 last_response 去重缓存**：与 /stats 同思路 —— 用户连发就是想看变化。`/tasks` 才需要去重（重复 list 长内容刷屏）。
- **不动 desktop /pinned slash 命令**：本 iter 仅 TG 端。desktop 已有「📌 N」chip + row chip 等视觉信号，没必要再加 slash。
- **不写 README 亮点**：与 `/pin` `/unpin` 同段已存在，本命令是该段的自然延伸；README 已说"桌面与 TG 之间形成派单 → 执行 → 状态管理 → 回传的闭环"，pinned 是这闭环的子动作。

## 验证

- `cargo test --lib` ✓ **1000 / 1000 通过**（+3 新测试：parses_pinned / format_pinned_tasks_list_empty_teaches_pin_command / format_pinned_tasks_list_groups_by_status_and_counts；registry coverage 测试名单加 "pinned" 仍通过）
- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.22s
- 改动 ~160 行（enum + name + title + registry zh/en + parser + help_text + format_pinned_tasks_list + bot handler ~110 + tests ~50）；既有 /tasks / /stats / format_tasks_list / read_tg_chat_task_views 路径完全不动。

## TODO 状态

6 条候选 auto-proposed 已完成 4 条，余 2 条留池：
- session 下拉按月份分组折叠
- 桌面 ChatPanel ⌘K 任务 ref picker

## 后续

- 用户在 TG `/pinned` 中点 task 编号能直接 `/done N` —— 复用 last_tasks_titles 缓存机制（与 /tasks 同），让"看 → 操作"链路 0 摩擦。
- `/pinned all` flag 显跨 chat 全集（需考虑跨 chat 数据泄露语义）。
- pinned task 自动通知：宠物在 proactive turn 完成 pinned task 时主动 TG 发"📌 我做完你钉的「X」了"特殊提示，与普通任务回流区分。
