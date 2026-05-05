# 长任务心跳 — 开发计划

> 对应需求（来自 docs/TODO.md「已确认」）：
> 长任务心跳：任务运行超过阈值未完成时主动汇报进展，避免静默卡死或被遗忘。

## 目标

让 pending 状态的 butler_tasks 不会在队列里"静默淤积"。当一项任务**被宠物动过手** 但**停滞超过阈值（默认 30 分钟）**时，下次 proactive turn 在 prompt 里**显式点名**这条任务，要求 LLM 要么写一句进展（`memory_edit update`），要么改成 `[done]` / `[error: 原因]`。

不引入新的"运行中"状态机 — 完全靠现有 `butler_tasks.updated_at` 推断：
- `updated_at == created_at` → 任务还从未被触碰过（不是"在飞"，是"还没启动"）
- `updated_at > created_at` → LLM 已经至少 update 过一次，进入"在飞"状态
- `now - updated_at ≥ threshold` → 心跳触发

这个推断是无侵入的：现有所有 LLM 路径只要走 `memory_edit`，`updated_at` 就会自动前进；不需要 LLM 学新协议。

## 非目标

- 不改 reactive chat 的行为 — 心跳仅在 proactive turn 里推送给 LLM。
- 不在面板里弹 toast / 红点提示用户 — v1 里宠物自己提及就够；用户已经能在「任务」页看到 updated_at。
- 不引入"硬中断"式催促 — 心跳是一条普通 hint，让 LLM 自己决定怎么承接（用户当前可能在专注 / mute，仍要尊重）。
- 不改 `butler_tasks_hint` 现有排序 — 心跳是补充信号，不替代任务列表。

## 设计

### 心跳判定（pure）

```rust
fn is_heartbeat_candidate(
    description: &str,
    created_at: &str,         // ISO local time-zone
    updated_at: &str,
    now: NaiveDateTime,
    threshold_minutes: u32,
) -> bool
```

返回 `true` 当且仅当：
1. `classify_status(description) == TaskStatus::Pending`（已存在的 task_queue helper）
2. `parse_local_dt(updated_at)` 与 `parse_local_dt(created_at)` 都解析成功
3. `updated_at - created_at >= TOUCHED_EPSILON_SECS`（默认 5s）— LLM 真的 update 过一次
4. `now - updated_at >= threshold_minutes 分钟`

`threshold_minutes == 0` → 始终返回 false（用户禁用）。

### 提示文本（pure）

```rust
fn format_heartbeat_hint(titles: &[String], threshold_minutes: u32) -> String
```

- 空列表 / 阈值 0 → 空字符串
- 1 条 →「你正在做的「{title}」已经超过 {N} 分钟没动了。请这一轮要么写一句进展（用 `memory_edit update` 更新描述里加状态），要么标记 `[done]` / `[error: 原因]`，别让用户的任务在队列里悄悄烂掉。」
- 多条 → 列出来加一段同样的 instruction

### Wiring

- `src-tauri/src/task_heartbeat.rs`（新文件）— 装上面两个纯函数 + 单测
- `proactive.rs::build_task_heartbeat_hint(now, threshold)` — IO 层：读 butler_tasks → 过滤 → 调 format。返回 redacted 字符串
- `PromptInputs.task_heartbeat_hint` — 新增字段
- `build_proactive_prompt` — 紧跟 `butler_tasks_hint` 推入

### 配置

`ProactiveConfig` 加一个字段：

```rust
#[serde(default = "default_task_heartbeat_minutes")]
pub task_heartbeat_minutes: u32,  // 0 = 禁用；默认 30
```

不挤进面板设置 UI（默认 30 分钟对绝大多数任务是合理值）；配置文件 yaml 直改即可。日后用户反馈说想在 UI 调再加。

## 阶段划分

| 阶段 | 范围 | 状态 |
| --- | --- | --- |
| **M1** | `task_heartbeat.rs` 纯模块 + 单测 | ✅ 完成（12/12 单测） |
| **M2** | settings 字段 + `build_task_heartbeat_hint` IO 层 + 接入 PromptInputs / build_proactive_prompt | ✅ 完成 |
| **M3** | 收尾（README / TODO / done/） | ✅ 完成 |

## 复用清单

- `task_queue::classify_status` / `TaskStatus`（pending 判定）
- `proactive::butler_schedule::parse_updated_at_local`（已存在的 ISO → NaiveDateTime 解析；可能需要 pub）
- `commands::memory::memory_list("butler_tasks")` + `.items[*]`
- `redaction::redact_with_settings`（与其他 hint 同样过滤）

## 待用户裁定的开放问题

1. **阈值默认值** 30 分钟是否合理？太短会过度打扰，太长就失去意义。30 在桌面陪伴节奏里大致是"一两个 proactive tick 内仍能起作用"的下限。
2. **UI 暴露**：本轮不做。如果心跳频繁触发但 LLM 应对得体，就保持隐式；如果用户想关 / 想调，下一轮再加面板字段。
3. **要不要也覆盖"created_at + 长时间却从未触碰"** 的场景？暂不 — 那是"积压未启动"问题，与本需求"运行中超时"语义不同；积压本就在 butler_tasks_hint 里通过 updated_at 升序排在最上方，宠物会自然取单。

## 进度日志

- 2026-05-04 15:00 — 创建本文档；准备进入 M1。
- 2026-05-04 15:30 — M1-M3 一次性合到 main：
  - **M1**：`src-tauri/src/task_heartbeat.rs` 落地 `is_heartbeat_candidate(description, created_at, updated_at, now, threshold)` + `format_heartbeat_hint(titles, threshold)`，全是 pure。12 条单测覆盖 threshold=0 / done / error / 未触碰 / TOUCHED_EPSILON 边界 / 阈值边界 / 阈值内跳过 / 时间戳坏数据，以及单 / 复数措辞、空列表。复用 `task_queue::classify_status` 与 `proactive::parse_updated_at_local`（后者从 `butler_schedule.rs` 提为 pub，通过 `proactive` 的 glob `pub use` 暴露给本模块）。
  - **M2**：`ProactiveConfig.task_heartbeat_minutes`（默认 30，0 = 禁用）；`proactive.rs::build_task_heartbeat_hint(now, threshold)` 在 0 时直接短路。`PromptInputs.task_heartbeat_hint` 新增字段，`build_proactive_prompt` 在 `butler_tasks_hint` 之后 push。`run_proactive_turn` 读 settings 拿阈值并构建 hint。`gate.rs` 里的测试 fixture 同步加上字段（默认 0，与 gate 逻辑解耦）。前端 `useSettings.ts` / `PanelSettings.tsx` 也加了类型与默认值，保持 settings round-trip 一致。
  - **M3**：README 在「自然语言派单」条目下方加亮点；`docs/TODO.md` 移除条目；本文件移入 `docs/done/`。`cargo test --lib` 691/691，`tsc --noEmit` 干净。
- **开放问题答复**：
  - Q1 默认 30 分钟：本轮先用 30 看实战效果。如果反馈"宠物催太勤"再考虑加 1.5x / 2x 的"后退"系数（被 LLM ack 后下次心跳间隔翻倍）。
  - Q2 UI 暴露：本轮不做。yaml 直改即可；如果用户在面板上明确要调再补 PanelSettings 字段。
  - Q3 是否覆盖"未启动积压"：不做。butler_tasks_hint 已经按 updated_at 升序展示，老任务自然冒上来；硬性心跳化反而会把"用户随手记的事"误判为"宠物在做"。
