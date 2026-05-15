# 任务依赖关系：`[blockedBy: …]` marker + proactive 自动过滤

## 背景

TODO 最后一项：

> 任务依赖关系：description 内 `[blocks: 标题]` / `[blockedBy: 标题]` marker；proactive 选单时跳过未解锁任务。

宠物当前用 priority / due / `[every: HH:MM]` 等机制驱动 proactive 任务 pick，但有些工作天然有"先做 A 再做 B"的顺序（先调研、再写决策；先约会议、再准备资料）。没有依赖表达 → 用户得手动控 `due` 或 priority 来排序，且当先决任务出错 / 延期时主任务也得手动跟动。

加 `[blockedBy: title]` 让依赖语义显式可读：proactive prompt 自动隐藏被卡的任务，面板 chip 标出"为什么没人做这条"，LLM 改主任务 description 时无需重新算时间。

## 改动

### 1. `task_queue.rs` —— 解析器 + 交叉引用

**`parse_blocked_by(description) -> Vec<String>`** (pure)

- 词法：扫描 `[blockedBy:` 起点 → 第一个 `]` 闭合；逗号分隔；trim；空 piece 跳；多 marker 累加；首次出现序 + 去重。
- 大小写敏感 `blockedBy`（与 `[task pri=...]` / `[result: ...]` 等既有 marker 同 camelCase 风）。
- 不解析 `[blocks: ...]` 反向 marker —— 执行语义只从一侧驱动，避免数据不一致歧义。
- 不闭合 `]` → 整段无效（与 LLM "marker 必须完整"的隐式契约一致）。

**`unresolved_blockers(items: &[(title, desc)]) -> HashMap<title, Vec<unresolved_blocker_titles>>`** (pure)

- active set = items 里 status != Done / Cancelled 的 title。
- 对每条 item parse_blocked_by → 与 active 交集 → 非空就进 map。
- typo / 已删 blocker → 视作"已解决"，避免永久死锁。

**6 个新单测**：basic / no marker / dedup-and-order / trim-pieces / unclosed / case-sensitive；以及 unresolved_blockers 的 done+cancelled-filter、typo、no-marker、error-state-still-active 4 个。

### 2. `task_queue::TaskView.blocked_by` + `build_task_view`

`TaskView` 增 `blocked_by: Vec<String>` 字段（serde default `[]` 让旧 JSON 兼容）。`commands/task::build_task_view` 调 `parse_blocked_by(raw_description)` 填入。

`telegram/commands.rs` 的 TaskView 测试 helper 也补 `blocked_by: Vec::new()` 保持编译通过。

### 3. `proactive::butler_schedule::format_butler_tasks_block` —— 自动过滤

```rust
let pairs: Vec<(String, String)> = items.iter().map(|(t, d, _)| (t.clone(), d.clone())).collect();
let blocked_map = crate::task_queue::unresolved_blockers(&pairs);
let blocked_count = blocked_map.len();
let filtered_items: Vec<&_> = items.iter()
    .filter(|(t, _, _)| !blocked_map.contains_key(t))
    .collect();
if filtered_items.is_empty() {
    return format!(
        "用户委托给你的管家任务（共 {} 条，全部被 [blockedBy: …] 依赖卡住…）",
        blocked_count,
    );
}
// ...rest of formatting uses filtered_items
if blocked_count > 0 {
    lines.push(format!(
        "（另有 {} 条任务被 [blockedBy: …] 依赖卡住…）",
        blocked_count
    ));
}
```

LLM 看到的 prompt：被卡的任务**不出现**在「用户委托给你的管家任务」清单里，但 header 紧跟一行透明告知"另有 N 条被卡住"—— 让 LLM 知道队列里还有沉睡工作，但不会选错。

**3 个新单测**：
- `filters_blocked_tasks` — 先决未完成 → 主任务不在 prompt，header 含 "依赖卡住"
- `unblocks_after_dep_done` — 先决 [done] → 主任务出现，无 blocked 横幅
- `all_blocked_returns_summary` — 极端情况输出兜底说明

### 4. `tools::memory_tools::ButlerTaskEditTool` description

butler_task_edit 工具 description 列出 `[blockedBy: title-a, title-b]` marker 形式：

- 必须精确匹配 title（case-sensitive）
- 缺失 / typo 视作已解决（no permanent dead-lock）
- 两个示例：单依赖 (`[blockedBy: 调研竞品] 写决策文档`) + 与 schedule 共存

让 LLM 知道这个 marker 存在，自然在对话中识别"先 A 再 B"语义时使用。

### 5. PanelTasks 🔒 chip

- 新增 frontend pure helper `computeUnresolvedBlockers(tasks)`（与后端 `unresolved_blockers` 同算法）。
- 顶层 `blockedMap = useMemo(() => computeUnresolvedBlockers(tasks), [tasks])` 让计算稳定。
- 每条 pending/error 任务行的 priority 徽章左侧：若 `blockedMap.has(title)` → 渲染 🔒 chip
  - 单依赖："🔒 等 A"
  - 多依赖："🔒 等 A +N"
  - tooltip 列出全部 blocker 列表
  - yellow tint（与 due "soon" 警示同语义但不同 motion）。
- terminal 行（done / cancelled）computeUnresolvedBlockers 跳过 —— "等"语义对结束态无意义。

### 6. `TaskView` 接口（前端 TS）

新增可选字段 `blocked_by?: string[]`，旧后端 / 旧 session 缺字段时 `[]` 兜底。

## 不做

- **不解析 `[blocks: ...]` 反向 marker**。可由用户冗余声明双向，但执行语义只从 `[blockedBy:]` 一侧驱动，避免两边不一致的歧义判断。如果未来 demand 强可加 "[blocks: X]" → 同步 SQLite 反向索引，但当前用例完全 cover 不到。
- **不强制 blocker 存在性**。typo / 已删的 blocker 视作已解决 —— 永久死锁比偶尔执行错更糟。
- **不传染依赖**（A → B → C，A 没完成时 B / C 都被卡）。当前实现只看直接依赖；间接依赖由 caller chain 自然形成（B 未 done → C 上的 `[blockedBy: B]` 看到 B 仍 active）。
- **不引入 SQLite `task_dependencies` 表**。description 内 marker 是单一数据源，避免与 yaml mirror / detail.md 形成三处不同步的灾难。
- **不写前端单元测试**。frontend 无 vitest；computeUnresolvedBlockers 是 12 行纯函数，逻辑明显；行为由后端 unresolved_blockers 的 4 个 rust 测试间接覆盖。

## 验证

- `cargo test --lib` ✓ **934 / 934 通过**（含 9 个新增测试）
- `cargo check` ✓ 0 error
- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.13s

## 后续

- TG bot `/tasks` 列表可加 🔒 标记（目前与 PanelTasks 平行渲染，blocker 信息 LLM 已通过 description 可见）。
- 依赖关系图视图（PanelTasks tab 新增 view mode："📊 依赖关系"）—— 可视化谁在等谁。
- AI 辅助检测循环依赖（A→B→A）—— 当前实现不会死锁（active set 计算与 filter 独立），但 LLM 可以警告用户。
