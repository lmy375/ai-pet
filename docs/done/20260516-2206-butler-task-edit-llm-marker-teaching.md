# butler_task_edit LLM 工具 description 加 weekday-set / reminderMin / silent marker 教学

## 背景

iter #193/#199 加 `[silent]`、iter #190 加 `[reminderMin: N]`、iter #204 加 `[every: 工作日/周末/周一 HH:MM]` weekday-set —— 三个新 marker 完整跨端覆盖 panel + TG bot。

但 LLM 在替 owner plan / execute butler_task 时，工具 `butler_task_edit` 的 description 仍只列 `[every:]/[once:]/[deadline:]/[done]/[error]/[cancelled]/[result]/[blockedBy]/[snooze]` 老 marker。LLM 不知道新 marker 存在 → 即使 owner 说"周末整理" 也写成 `[every: 10:00]` 加文字 hint 而非 `[every: 周末 10:00]`；想给 "开会前提醒一下" 时不会用 `[reminderMin: 5]`；不知道 owner 标 `[silent]` 是显式意图（不能自由覆盖）。

补全工具 description 让 LLM 自然产出新 marker。

## 改动

### `src-tauri/src/tools/memory_tools.rs` — `butler_task_edit` 工具 description

#### 1. Schedule prefix 段扩 weekday-set

```
- `[every: HH:MM] topic` — daily recurring at local HH:MM
- `[every: 工作日 HH:MM] topic` — recurring Mon-Fri only (weekday-set)
- `[every: 周末 HH:MM] topic` — recurring Sat-Sun only
- `[every: 周一 HH:MM] topic` — single weekday (周二/周三/.../周日; EN Mon/Tue/.../Sun)
- `[once: ...]` / `[deadline: ...]` 不变
```

#### 2. Markers 段加 reminderMin / silent

```
- `[reminderMin: N]` — soft reminder N minutes (1-1440) before fire-time.
  Pairs with [once:]/[deadline:]/[every:]. Desktop pet ChatMini surfaces quiet
  "🔔 提醒：「X」将在 N 分钟后到点" toast at that lead time WITHOUT
  opening the proactive Live2D mode. Useful for "准备会议材料" /
  "开会前 5 分钟提醒一下" scenarios.

- `[silent]` — owner explicitly opted this task out of YOUR auto-pick
  queue (proactive cycle skips it; still visible in panel + manually
  triggerable). Honor it: if the owner already asked you not to pick
  this one, leave it alone unless they say otherwise. Don't add
  [silent] yourself on tasks the owner expects you to handle.
```

后者尤其重要 —— LLM 必须**尊重 owner 的 [silent] 意图，不主动 unset**。

#### 3. Examples 段加 3 个

```
Example weekday-set: [every: 工作日 09:30] 早上 standup 准备 — Mon-Fri only.
Example reminder: [once: 2026-05-20 18:00] [reminderMin: 5] 准备会议材料 — fires at 17:55 a soft buffer toast.
Example stacked: [every: 工作日 09:30] [reminderMin: 3] standup 准备 — both weekday-set schedule and a 3-min lead-time reminder apply.
```

## 关键设计

- **教学性 + 行为约束并行**：weekday-set / reminderMin 是新可用 marker，silent 是"识趣不动" 行为约束 —— 两者都加描述清楚 LLM 怎么用 / 何时不该用。
- **示例覆盖典型场景**：standup（工作日 + 提醒）/ 会议（once + 提醒）/ stacked（weekday-set + reminderMin 一起）—— 让 LLM 看完直接复用 pattern。
- **honor [silent] 显式 instruction**：直接告诉 LLM "don't add silent yourself on tasks the owner expects you to handle"。silent 是 owner-intent marker，LLM 不应自由 toggle。
- **不修改其它工具**：todo_edit / memory_edit 不涉及这些 marker，scope 限定 butler_task_edit。

## 不做

- **不暴露 silent 作为 enum 字段**：LLM 仍通过 description 字面量写 marker。引入 schema 字段会 lock LLM 进 "可自由 toggle silent" 心智 —— description 字面量天然 ad-hoc，less 自由触发。
- **不写测试**：description 是文本说明给 LLM 看；没办法 unit test "LLM 用 marker 正确"。视觉验证 = owner 实际用 chat 让 LLM plan 一个"周末整理" 任务，检查 LLM 是否写出 `[every: 周末 ...]`。
- **不更新 README**：description 是给 LLM prompt 的，不是用户文档。

## 验证

- `cargo check` ✓ (7 既有 warning，无新 error)
- 改动 ~10 行 schema text（description 段扩 6 行 + 3 example 行 + 几个标点）。既有 schema parameters / butler_task_edit_impl / 工具注册路径完全不动。

## TODO 状态

剩 3 条留池：
- PanelTasks "新建任务" + ⇧Enter 创建并立即打开 detail 编辑器
- PanelMemory ai_insights 类目顶 "🧠 由宠物自己写" banner
- 桌面 pet 右键加「⏰ 设倒计时 N 分钟 nudge」

## 后续

- todo_edit 工具描述也补 `[reminderMin: N]` —— 但 todo 不进 proactive cycle，需考虑语义边界。
- 加 `[silent]` 的 honored test：consolidate 流程 / 任何 LLM-driven marker rewrite 都不该误剥 owner 标过的 [silent]。
- 工具描述自动生成 + 多语言切换（中英都给 LLM 看）—— scope creep。
- 加一段 description "如何写好 description 给后续 LLM run 看"（让 LLM 自己写 task 时也考虑下一次 prompt 读取的清晰度）。
