# 任务 snooze 标记：`[snooze: YYYY-MM-DD HH:MM]` + proactive 自动暂停

## 背景

TODO（auto-proposed 上一轮）：

> 任务 snooze 标记：description `[snooze: YYYY-MM-DD HH:MM]` 让 proactive 选单临时忽略；到点自动重现，与 blockedBy 同语言但时间维度。

`[blockedBy: …]`（20260514-1211）解决了"先做 A 再做 B"的逻辑依赖维度。但有些"先放着"语义不是逻辑依赖而是时间相关 —— "这个 idea 等下个 sprint 启动"、"周末再说"、"今天专注别的"。强行用 blockedBy 表达需要造一个 fake "稍后再说" 占位任务；不如直接给时间维度一个 marker。

设计语言与 blockedBy 同：description-embedded marker、解析为 pure 函数、proactive prompt 同一处 filter、面板 chip 同一处渲染、`butler_task_edit` 工具描述同处说明。

## 改动

### 1. `task_queue.rs` — 解析器 + 交叉引用

**`parse_snooze(description) -> Option<NaiveDateTime>`**

- 词法：扫 `[snooze:` 起点 → 第一个 `]` 闭合；inner trim 后用 `chrono::NaiveDateTime::parse_from_str` 严格解析 `%Y-%m-%d %H:%M`。
- **多个 marker 取最后一个有效值**：让 LLM 重新 snooze 时可以直接 append `[snooze: 新时刻]`，不必先剥旧 marker。这点与 blockedBy 不同（blockedBy 是合并所有 entries）—— 这里"最新时刻覆盖旧时刻"才是用户意图。
- 字段值越界 / 非时间串 / 不闭合 / 大小写错 → 跳过该条 marker（chrono parse 对零填充宽松，但对越界值严格）。
- 与 `[once: …]`（butler_schedule）协议同形（24h、本地时区、minute precision），减少 LLM 心智成本。

**`snoozed_until_map(items, now) -> HashMap<title, until>`**

- 过滤过去时刻：`until > now` 才算"仍 snooze"；`now == until` 即唤醒（严格 future）。
- 与 `unresolved_blockers` 同 surface，proactive layer 一次 union filter 两者。

**9 个新单测**：basic / no-marker / multiple-takes-latest / invalid / invalid-then-valid / case-sensitive / unclosed / map-filters-past / map-boundary-now-equals-wake-is-awake。

### 2. `TaskView.snoozed_until: Option<String>` + `build_task_view`

`TaskView` 新增字段（serde default None）。`build_task_view` 调 parse_snooze → 若仍 future 渲染 `YYYY-MM-DDThh:mm` 字符串（与 `due` 协议同形），过点 → None。**过点检查在 backend 完成 → 前端 `t.snoozed_until` truthy 即可直接显 💤 chip，不必再算时间差。**

`telegram/commands.rs` 的 TaskView 测试 helper 补 `snoozed_until: None`。

### 3. `format_butler_tasks_block` — proactive 自动 filter

紧贴 blockedBy filter，多 一道 snooze filter。两个 map 一起 OR 起来过滤 items：

```rust
let blocked_map = crate::task_queue::unresolved_blockers(&pairs);
let snooze_map = crate::task_queue::snoozed_until_map(&pairs, now);
let filtered_items: Vec<&_> = items.iter()
    .filter(|(t, _, _)| !blocked_map.contains_key(t) && !snooze_map.contains_key(t))
    .collect();
```

Header 透明告知 LLM 队列里还有 N 条 blocked + M 条 snoozed，两者都 > 0 时用顿号合一行：

```
（另有 1 条被 [blockedBy: …] 依赖卡住、2 条处于 [snooze: …] 暂停期，先决条件解决 / 时刻到达后才会出现在本列表。）
```

全 unavailable 极端情况（filtered_items empty）兜底句子分 3 种 reason 文案（blocked-only / snoozed-only / both）。

**3 个新单测**：snoozed-hidden、past-snooze-passes-through、blocked+snoozed-header 两段都列。

### 4. `butler_task_edit` tool 描述

紧贴 `[blockedBy: …]` 段加一段 `[snooze: …]` 说明：

- 协议（YYYY-MM-DD HH:MM，本地时区，minute precision）
- 自动过期不需要 cleanup
- 重新延后追加新 marker 即可（不必剥旧）
- 一个 example

### 5. Frontend 💤 chip

`TaskView` interface 加 `snoozed_until?: string | null`。

在 PanelTasks 行渲染处，🔒 chip 之前加 💤 chip：

- 仅 `t.snoozed_until` truthy 且 `t.status !== "done" && t.status !== "cancelled"` 才显（终态行暂停语义无意义）。
- 紫色 tint（与 yellow 🔒 区分）。
- 文字 "💤 至 MM-DD HH:MM"（13 字符以内不拉宽 row）；hover tooltip 显完整 `YYYY-MM-DD HH:MM`。
- "过点 → 不显" 由 backend filter 保证，前端只判 truthy。

## 不做

- **不加 `/snooze` slash 命令**。用户 / LLM 直接通过 butler_task_edit 改 description 即可；新增 slash 多一道 IPC 层 + 重复参数解析，价值不大。后续如果证明有需求可以加。
- **不在 PanelTasks 加"snooze 按钮"**。task 行右键菜单 / 编辑 modal 已经能改 description；UI 加按钮先要解决"snooze 到哪天" 的 picker UX，不是本轮的重点。
- **不让 snooze 与 due 联动**。两者语义独立：due 是承诺时刻，snooze 是"暂时不要烦我"；同一任务可有 due 在远期且 snooze 在近期。
- **不强制 snooze marker 唯一**。多 marker 取最新值是设计 feature（让 LLM 简单 append），不是 bug。
- **不写 frontend unit test**。前端无 vitest；chip 渲染条件简单 truthy 判断 + 状态门，后端测试已 cover 核心算法。

## 验证

- `cargo test --lib task_queue::tests::parse_snooze` ✓ 7/7
- `cargo test --lib task_queue::tests::snoozed_until_map` ✓ 2/2
- `cargo test --lib proactive::butler_schedule::` ✓ 55/55（含 3 新增 + 1 既有 header 文案更新）
- `cargo test --lib` ✓ **955 / 955 通过**（952 → 955，net +12 包括 snooze + filter 测试，1 既有 blocked 测试改了 header 字面量）
- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.12s

## 后续

- 自动 cleanup 过期 snooze marker：consolidate 循环里 sweep 一次把所有 `[snooze: 过去时刻]` 从 description 剥掉。当前 marker 不影响过期后行为（filter 已忽略），只是会让 description 越来越长。
- TG / chat slash `/snooze <title> <duration>`：把"暂停 30 分钟"/"暂停到明早 9 点"做成自然语言。
- 面板 task 行右键菜单加 "💤 暂停到 …" 子菜单（today 18:00 / tomorrow 09:00 / next monday 09:00 / +1 week）。
