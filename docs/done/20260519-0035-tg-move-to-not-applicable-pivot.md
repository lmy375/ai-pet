# TG bot `/move_to <title> <category>` — backend constraints make TG entry not applicable（iter #516）

## Discovery

TODO 提案：「TG bot `/move_to <title> <category>` 命令：跨 memory category
迁移 — 复用既有 `memory_move_category` Tauri 命令」。

实际 grep + 读 backend `memory_move_category`（src/commands/memory.rs:708）
发现该命令**明确拒绝 mirrored categories**：

```rust
let is_mirrored = |c: &str| {
    matches!(c, "butler_tasks" | "todo" | "task_archive" | "ai_insights")
};
if is_mirrored(&old_category) {
    return Err(format!(
        "Cannot move from mirrored category '{old_category}'. \
Mirrored kinds (butler_tasks/todo/ai_insights/task_archive) need explicit \
migration to preserve queue / archive state — use the kind's own delete + \
new-category create flow instead."
    ));
}
if is_mirrored(&new_category) { return Err(...); }
```

**TG-scope tasks 全部在 `butler_tasks`** — `resolve_tg_task_title` 只查
butler_tasks chat-scope view，索引解析也只从 butler_tasks 行号。意味着
任何 TG `/move_to <title> ...` 命令都会被 backend 拒。

backend 的设计原则合理：mirrored cats 携带 SQLite mirror + queue state /
archive state / decision_log 关联，简单 yaml move 会破一致性。手动迁
移流程是「在 source kind 走 delete + 在 target kind 走 create」。

## Decision

**不实现 TG `/move_to`** — backend 约束让 TG-scope use case 无法落地。
理论上可加 TG 命令支持「非 mirrored cat 之间 move」（如 `general →
journal`），但：

1. TG 没有非 butler_tasks 的 title resolve 路径（既有 try_resolve_by_
   index + resolve_tg_task_title 都是 butler-only）
2. owner 在 TG 端通常聊任务（butler_tasks），少有跨 cat 迁移非任务 memory
   item 的需求
3. 桌面端 PanelMemory 已有「drag-drop item 跨 cat」UI（既有）— 鼠标
   场景已覆盖

如未来出现明确用例（如「TG /move_to <general_title> <journal>」），可
单独建命令。当前 TODO 项删除。

## Cross-reference

- PanelMemory drag-drop 跨 cat：已实现既有路径
- mirrored cat 迁移走「kind 自身 delete + new cat create」— 桌面端
  对应 PanelTasks「+ 新建」+ 老任务 /cancel
- `memory_move_category` backend 仍可用于 non-mirrored → non-mirrored 场
  景（未来如果加自定义 cat 间迁移场景）

## Verification

无代码改动。TODO 项删除，本 doc 作记录。

procedure 改进：本 cycle 第 7 个 already-implemented / not-applicable pivot
（与 #495 #498 #499 #500 #506 #516 同教训）。propose TODO 前不仅要 grep
代码存在性，还要 read backend 约束（pub fn 拒绝条件 / mirrored kinds /
permission gates 等）— 表面 API 存在不代表 use case 能 work。
