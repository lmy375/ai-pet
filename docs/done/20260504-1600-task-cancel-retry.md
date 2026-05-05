# 任务取消与重试 — 开发计划

> 对应需求（来自 docs/TODO.md「已确认」）：
> 任务取消与重试：失败任务一键重跑，运行中可取消并把原因写入决策日志。

## 目标

给「任务」面板加两个动作：
1. **重试**：`error` 状态的任务一键重跑 — 剥掉 `[error: ...]` 标记，状态回到 `pending`，下次 proactive turn 自然取单。
2. **取消**：`pending` / `error` 状态的任务一键终结 — 在描述里写入 `[cancelled: 原因]`，状态变 `cancelled`，从活动队列里淡出；同时把"取消时刻 + 标题 + 原因"写进 decision log，供未来调试 / 周报回看。

不引入额外存储后端 — 与已有 task header / `[error: ...]` / `[done]` 同形，通过描述字符串携带状态，让 LLM 同样的 memory_edit 路径继续是真相源。

## 非目标

- 不做"运行时硬中断" — 当前架构里宠物没有"长进程"概念，每次 proactive turn 都是无状态的；"运行中"只是 description 已被触碰过的语义状态。"取消"在我们这里 ≈ "在它被取走之前先打上 cancelled 标记，下一轮 LLM 会跳过"。
- 不在桌面气泡里加按钮 — 操作面集中在面板「任务」标签页，气泡保持极简。
- 不区分"用户取消"与"系统取消"— 都通过 `task_cancel` 命令，原因字段自由填。
- 不做批量取消 / 重试 — 单条操作已足够覆盖典型场景，批量等到积压成痛点再补。

## 设计

### 状态扩展：`TaskStatus::Cancelled`

`task_queue::TaskStatus` 增加 `Cancelled` 变体。`classify_status` 决策顺序更新为：

```
1. [cancelled  → Cancelled (含原因)
2. [error      → Error (含原因)
3. [done]       → Done
4. otherwise    → Pending
```

为什么 `Cancelled` 排最优先：用户的「取消」是终态意图 — 一旦标了 cancelled，无论之前是 error 还是 pending，都不应回到那两个状态。已 errored 的任务被取消 → 显示为已取消（不再可重试）；纯 pending 被取消同理。

`classify_status` 返回值升级：保持 `(TaskStatus, Option<String>)`，`Option<String>` 在 Cancelled / Error 时分别承载 cancelled / error 的原因。

### 排序比较器扩展

`compare_for_queue` 现有层级：error > pending > done。加入 cancelled 后：

```
error > pending > done > cancelled
```

cancelled 排最末（与 done 一起属于"已结束"段）— 面板默认隐藏；下一轮 proactive 也不会被取走。

### Tauri 命令

#### `task_retry(title: String) -> Result<(), String>`

- 在 butler_tasks 里找到 title 匹配的条目（重名时取最早一条）
- 当前 status 必须是 `Error`，否则返回 Err（避免把 done / cancelled / pending 误认为可重试）
- 把描述里所有 `[error...]` 段剥掉（也清掉孤立的 `[done]` 标记，防止 LLM 上次误标）
- `memory_edit("update", ...)` 写回；`updated_at` 自动前进 → 心跳计数也重置
- decision_log push `"TaskRetry"` + 标题

#### `task_cancel(title: String, reason: String) -> Result<(), String>`

- 找到 title 匹配的条目
- 当前 status 是 `Done` / `Cancelled` 直接返回 Err（已结束的任务不能再"取消"）
- 在描述末尾追加 `[cancelled: <trimmed_reason>]`；空 reason 写 `[cancelled]`（仍能被 detector 识别）
- decision_log push `"TaskCancel"` + `title — reason`

两个命令的实现都拆成 pure helper：
- `task_queue::strip_error_markers(desc) -> String` 给 retry 用
- `task_queue::append_cancelled_marker(desc, reason) -> String` 给 cancel 用

### Panel UI 调整

`PanelTasks.tsx`：
- error 行：显示「重试」按钮（点击 → invoke `task_retry`）
- pending / error 行：显示「取消」按钮（点击 → 行内展开 reason textarea + 「确认取消」/「不取消」）
- cancelled 行：显示「已取消 · {reason}」徽章；不再展示动作按钮
- 「显示已完成」开关改为「显示已结束」（包括 done + cancelled）— 文案更准确

### 决策日志接入

直接复用 `decision_log::DecisionLogStore::push(kind, reason)`，与 daily_review / proactive 等同源。format：
- `kind = "TaskRetry"`, `reason = "<title>"`
- `kind = "TaskCancel"`, `reason = "<title> — <reason or 用户未填>"`

### 兼容性

- 历史 cancelled 任务不存在（新概念）—— 无迁移负担
- 历史 error 任务遇到新版前端 → 直接显示「重试」按钮
- 旧前端（如缓存里有旧 PanelTasks）调用旧命令仍工作 —— 我们只新增命令，不改既有

## 阶段划分

| 阶段 | 范围 | 状态 |
| --- | --- | --- |
| **M1** | `task_queue.rs` 扩展 Cancelled / 排序 / classify / strip-helpers + 单测 | ✅ 完成（13 条新增单测，task_queue 38/38） |
| **M2** | Tauri 命令 `task_retry` / `task_cancel` + 注册 | ✅ 完成 |
| **M3** | 前端 PanelTasks 行内动作 + cancelled 徽章 + 文案 | ✅ 完成 |
| **M4** | 收尾：README + TODO + done/ | ✅ 完成 |

## 复用清单

- `task_queue::{classify_status, TaskStatus, compare_for_queue, TaskView}`
- `commands::memory::{memory_list, memory_edit, MemoryItem}`
- `decision_log::DecisionLogStore`
- `PanelTasks.tsx` 现有 status 徽章 + 「显示已完成」开关样式

## 待用户裁定的开放问题

1. **重试是否清掉 done 标记**：当前选「是」。如果 LLM 误把一条又 error 又 done 的任务标了 done（理论上不该出现），重试时也一并复位为 pending。
2. **cancelled 的 task 是否能 unCancel**：暂不做。一旦取消，建议用户重新建任务而不是 unCancel — 避免把"取消原因"留作历史误导后续 LLM。
3. **取消原因为空时的兜底文案**：当前选直接 `[cancelled]` 不带消息（被 classify 识别但 reason 为 None）；面板显示「已取消」无副文案。比写"用户未填原因"更干净。

## 进度日志

- 2026-05-04 16:00 — 创建本文档；准备进入 M1。
- 2026-05-04 16:45 — M1-M4 一次性合到 main：
  - **M1**：`task_queue.rs` 加 `TaskStatus::Cancelled` + `classify_status` 优先级 cancelled > error > done > pending；`status_rank` 把 cancelled 放最末；新 helpers `strip_error_markers`（单遍扫 + UTF-8 char-boundary 安全；测试发现的 bug 修复了之前残留 `]` 的实现）与 `append_cancelled_marker`。13 条新增单测覆盖 4 种 cancelled 形态（带原因 / 无原因 / 覆盖 error / 覆盖 done）+ strip 4 种场景 + append round-trip + 排序 cancelled 在 done 之后 / 不与 pending 争位。
  - **M2**：`commands/task.rs` 加 `task_retry(title, decisions)` 与 `task_cancel(title, reason, decisions)`；前者要求 status==Error，后者拒绝 Done/Cancelled。两条命令都 push DecisionLog（`TaskRetry: <title>` / `TaskCancel: <title> — <reason>`，无原因时只写 title）。lib.rs 注册。
  - **M3**：`PanelTasks.tsx` 加 cancelled 灰色徽章；行内「重试」按钮（仅 error 行）+「取消」按钮（pending/error 行）；点取消展开 reason 输入 + 「确认取消」/「不取消」副按钮；cancelled 行显示「取消原因：...」副文案；「显示已完成」开关重命名为「显示已结束（含已完成 / 已取消）」并把 `isFinished()` helper 抽出来集中过滤逻辑。
  - **M4**：cargo test --lib 704/704 通过；tsc 干净。README 加亮点；TODO 移除条目；本文件移入 `docs/done/`。
- **开放问题答复**：
  - Q1 重试清 done：保留这一行为。`strip_error_markers` 同时清 error / done，让重试后的任务回到干净 pending 状态。
  - Q2 unCancel：暂不做。一旦取消请重新建任务（用户可在面板照原任务复制粘贴 + 新建）。后续若用户反馈频繁误取消再加 undo 浮层。
  - Q3 空原因：直接写 `[cancelled]` 不带消息，前端 cancelled 徽章无副文案 — 比"用户未填原因"干净。
