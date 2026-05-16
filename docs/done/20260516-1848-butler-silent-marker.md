# butler_task `[silent]` marker — owner 标"知会存在但不要主动选"

## 背景

owner 有些 butler_task 想"保留记录但不希望 pet 主动催"：
- 偶尔记一下要做的事
- "想到某长辈"这种 owner 自己记得就行的任务
- 临时想搁置 + 后期可能恢复的事项

既有 `[snooze: ...]` 需要具体时刻（语义不符）；`[blockedBy: ...]` 需要依赖任务（hack 出"虚拟 blocker" 不优雅）。

加新 `[silent]` marker：owner 显式标"不要在 proactive cycle 主动选" —— 任务仍在 PanelMemory 可见，仍能手动在 PanelTasks 触发，但 LLM 完全看不到。

## 改动

### Backend

#### `src-tauri/src/task_queue.rs` — `parse_silent` helper

```rust
pub fn parse_silent(description: &str) -> bool {
    description.contains("[silent]")
}
```

严格字面 marker，与 `parse_pinned` 同模式。

#### `src-tauri/src/proactive/butler_schedule.rs` — filter + header

加入 `format_butler_tasks_block` 既有 filter pipeline（blocked / snooze 之后）：

```rust
let silent_set: HashSet<String> = items
    .iter()
    .filter(|(_, d, _)| crate::task_queue::parse_silent(d))
    .map(|(t, _, _)| t.clone())
    .collect();
let silent_count = silent_set.len();

let filtered_items = items.iter().filter(|(t, _, _)| {
    !blocked_map.contains_key(t)
        && !snooze_map.contains_key(t)
        && !silent_set.contains(t)
}).collect();
```

Header 两处更新：
- 全部 filter 掉时：增加 silent 分支并 join，单原因走口语化 "全部被 owner 标 [silent]"
- 部分 filter：增加 silent 段 "（另有 N 条被 [blockedBy: …] 依赖卡住、M 条 [snooze: …] 暂停期、K 条被 owner 标 [silent] 不选...）"

#### 两条新单测 in `butler_schedule::tests`

```rust
#[test]
fn format_butler_tasks_block_filters_silent_tasks() {
    // silent 任务从主 list 消失但 header 透明告知
    ...
    assert!(out.contains("活跃任务"));
    assert!(!out.contains("静默任务"));
    assert!(out.contains("1 条被 owner 标 [silent] 不选"));
}

#[test]
fn format_butler_tasks_block_all_silent_returns_special_msg() {
    // 全部 silent → 不挂主 list，仅特殊文案
    ...
    assert!(out.contains("全部被 owner 标 [silent]"));
    assert!(out.contains("共 2 条"));
    assert!(!out.contains("X")); assert!(!out.contains("Y"));
}
```

跑 `cargo test --lib proactive::butler_schedule` ✓ 60 passed（含 2 新）。

### Frontend

#### `src/components/panel/PanelMemory.tsx`

1. SCHEDULE_TEMPLATES 加 quick-insert 按钮：

```ts
{ label: "🔇 silent", text: "[silent] " },
```

2. butler_task 行显 🔇 silent chip（紧贴 reminderMin chip 前，与 schedule chip 共行）：

```tsx
{catKey === "butler_tasks" && /\[silent\]/.test(item.description) && (
  <span style={{ ...muted gray ... }} title="...解除：编辑描述删掉 [silent] marker">
    🔇 silent
  </span>
)}
```

3. butler_tasks placeholder 补 `[silent]` 示例：

```
或叠加 [silent] 让该任务知会存在但不被 LLM 主动选择...
  [silent] [every: 周日 16:00] 给某长辈打电话
```

### README

宠物管家 section 加新功能 bullet 解释 silent 语义 + 与 blockedBy / snooze 维度差异。

## 关键设计

- **filter pipeline 同源**：blocked / snooze / silent 都是 "not actionable now" 信号，进同 union filter。三者维度不同：blockedBy = 依赖（被动 / 等 dep done）；snooze = 时间（被动 / 等时刻到）；silent = owner 意图（主动 / 仅手动 remove）。
- **header 透明告知 LLM**：filter 后队列变短但 prompt header 列出 "另有 N 条被 X 卡住"，让 LLM 知道还有"沉睡"任务存在，不会以为"队列空了"。silent 段单独算 + 单独显，与 blocked/snooze 视觉对齐。
- **全部 silent 时特殊文案**："全部被 owner 标 [silent]（共 N 条），不在主动 cycle 里出现，等 [silent] marker 移除后再出现" —— LLM 不会以为系统坏了，理解为 owner 显式静默。
- **strict literal `[silent]`**：不接 `[silent: ...]` / `[Silent]` 等变体，与 `parse_pinned` 同 normalize 策略 —— 单一字面 form 让 UI 切换 / LLM 写入 / 用户手敲 三方都看到同一形态。
- **chip muted gray**：用 `--pet-color-border` bg + muted fg + 0.85 opacity，视觉上"低能见度" 与"被静默"语义一致。区别于 pinned amber / reminderMin green。
- **frontend regex inline 而不复用 backend parse_silent**：前端用一行 regex `/\[silent\]/`，避免引入更多 wasm / 转译 layer；backend 才是 SoT 决策点，前端只是渲染信号。
- **SCHEDULE_TEMPLATES 按钮 + placeholder 双教学**：让 owner 第一次编 butler_task 时就发现 marker 存在 + 示例自然解释场景。

## 不做

- **不支持 `[silent: reason]` 带原因变体**：第一版字面 form 够用；reason 可写进 description body。
- **不绑右键菜单一键 toggle silent**（与 `[pinned]` 的 `task_set_pinned` 命令对偶）：silent 是低频操作（owner 偶尔标 / 解），编辑描述删 marker 已够；不引新 Tauri 命令 + DB 更新路径。后续 iter 可加快捷按钮。
- **不让 silent task 被 telegram bot `/tasks` 隐藏**：PanelMemory 仍显该 task；TG `/tasks` 也是 owner 自己看的（与 LLM proactive 不同 lane）。
- **不在 task_completion_hint / build_butler_deadlines_hint 等其它 hint 中也 filter silent**：本 iter 仅切 proactive cycle 的"主动 pick" lane（format_butler_tasks_block）。deadlines hint 是 urgency 信号、completion hint 是 "刚完成" 报告，与 silent 维度无关。

## 验证

- `cargo test --lib proactive::butler_schedule` ✓ 60 passed（含 2 新单测）
- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.17s
- 改动 ~120 行（task_queue parse_silent 8 + butler_schedule filter+header 30 + 单测 50 + frontend chip 25 + template/placeholder 6）。`build_butler_tasks_hint` 主路径 IO 不变；blocked_map / snooze_map / sort / loop 完全保留。

## TODO 状态

剩 2 条留池：
- ChatMini bubble 底 "⏱ N 分前" hover chip
- PanelMemory item 行右键「📅 显创建时间」

## 后续

- 加 `task_set_silent` Tauri 命令 + PanelTasks 行右键菜单 "🔇 静默" toggle（与 `[pinned]` 的 `task_set_pinned` 对偶）。
- proactive cycle 偶尔（e.g., 每周一次）把 silent items 列给 LLM 看一次 "owner 有 N 条沉睡任务，要不要问下还要不要做"—— 防 silent 任务永久沉底被 owner 忘记。
- silent + `[every: ...]` 组合：owner 想"知道每周该做但不要主动催"。当前已工作（filter pipeline 共同生效），但可以加入 docs 例子。
- TG bot `/silent <title>` 命令一键 add/remove marker，与 `/pin` `/snooze` 形成完整 marker 命令族。
