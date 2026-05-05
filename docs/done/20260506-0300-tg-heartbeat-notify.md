# TG 心跳静默通知 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> TG 心跳静默通知：长任务 task_heartbeat 静默期超阈值时，TG bot 主动发一句"任务 X 卡 30 分钟了，要不要我点一下"，配套对应任务 cancel/retry。

## 目标

`run_task_watcher` 已经把 **TG-origin 任务的终态变更**（done / error / cancelled）
自动回传到原 chat。本轮加一个对偶信号：**stuck 状态**——pending 任务被宠物动
过手但 stagnated 超过 settings.proactive.task_heartbeat_minutes 阈值，TG bot
主动发消息提醒 + 给 `/cancel` `/retry` 操作提示。

复用现有 `task_heartbeat::is_heartbeat_candidate` 纯函数（已 9 条单测覆盖），
后端不做新逻辑，watcher 多读一次状态。

## 非目标

- 不做"一直重发"的轮询通知 —— 同一条 stuck 任务在 updated_at 不变期间只发一次。
  updated_at 变化（LLM 又写了一笔 / 用户改了 priority / due）→ 下次 stuck 时再发。
- 不在桌面气泡里推 —— 桌面已有 proactive prompt 注入（task_heartbeat_hint 让
  LLM 在下次主动开口里点名）。本轮只补"TG 那边收不到桌面 prompt"的盲区。
- 不写 README —— TG 心跳是已有桌面心跳的多端补强。

## 设计

### Watcher 内部状态

`run_task_watcher` 加一个 `HashMap<String, String>` ——
`last_heartbeat_notified: title → updated_at（上次通知时刻的 ts 字符串）`。

每 tick：
1. 现有逻辑：终态变更通知（保留）
2. 新增 heartbeat 扫描：
   - 读 `settings.proactive.task_heartbeat_minutes`；0 → 跳过
   - `now = chrono::Local::now().naive_local()`
   - 对每条 TG-origin 任务：
     - 若 `is_heartbeat_candidate(desc, created_at, updated_at, now, threshold)` && 
       `last_heartbeat_notified.get(title) != Some(updated_at)`：
       - 发消息 → record `last_heartbeat_notified[title] = updated_at.clone()`
3. cleanup：把 `last_heartbeat_notified` 里 title 已不在当前 snapshot 的条目剔
   掉（避免删任务后内存泄漏）

### 冷启动

与终态通知一致：第一次 pass 不发（`first_pass` 标志已存在），只填充 snapshot。
heartbeat 也走同 first_pass gate—防止重启就把所有"早就 stuck"的任务再轰炸一遍。

### 通知文案

```
⏳ 任务「{title}」卡了 {N} 分钟没动了，要不要我点一下？
回 /retry {title} 让我重试 · /cancel {title} 取消
```

抽 pure helper `format_heartbeat_message(title, minutes)`，便于单测。

### 测试

新增单测覆盖：
- `format_heartbeat_message` 文案契约（含 title / minutes / cancel 与 retry 命令）

`is_heartbeat_candidate` 纯函数已有 9 条单测，watcher 集成路径不写集成测试
（涉异步 runtime + bot mock，成本不值；与现有 `run_task_watcher` 终态通知
路径同处理）。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | `format_heartbeat_message` pure helper + 单测 |
| **M2** | `run_task_watcher` 加 heartbeat 扫描 + last_notified map + cleanup |
| **M3** | cargo test + cleanup |

## 复用清单

- `task_heartbeat::is_heartbeat_candidate`（已有，9 条单测）
- 既有 `parse_task_origin` / `classify_status` / settings access pattern
- `bot.send_message` IO 路径

## 待用户裁定的开放问题

- threshold 与桌面心跳共享（settings.proactive.task_heartbeat_minutes）vs 独
  立 `tg_heartbeat_minutes`？本轮**共享**——心跳是任务级语义，多通道无理由
  用不同阈值；如未来要独立再加。
- 通知消息里要不要带 chat-side 的 chat_id 校验？复用现有 `parse_task_origin
  → TaskOrigin::Tg(chat_id)`，发到对应 chat_id 即可。

## 进度日志

- 2026-05-06 03:00 — 创建本文档；准备 M1。
- 2026-05-06 03:20 — 完成实现：
  - **M1**：`telegram/bot.rs` 加 `format_heartbeat_message(title, minutes)` pure helper（`⏳ 任务「X」卡了 N 分钟没动了，要不要我点一下？回 /retry X 让我重试 · /cancel X 取消`）。2 条新增单测覆盖 title / minutes / 命令模板存在性 + title 去空白。
  - **M2**：`run_task_watcher` 加 `last_heartbeat: HashMap<String, String>`（title → 上次发心跳通知时的 updated_at）；每 tick 重读 `settings.proactive.task_heartbeat_minutes`（用户改设置无需重启）；对每条 TG-origin 任务，复用既有 `task_heartbeat::is_heartbeat_candidate` 纯函数；同 (title, updated_at) 组合只发一次（updated_at 变即视作"任务又活了一下"，下次 stuck 再发）；终态时移除 last_heartbeat 条目防止 cancel 后任务复活的死锁；cleanup 步骤 `retain(seen_titles)` 移除已删任务的悬空条目。
  - **M3**：first_pass gate 与既有终态通知共享，冷启动只填 snapshot 不发任何消息（避免重启即骚扰）。
  - **M4**：`cargo test --lib` 887/887（+2）。TODO 移除条目；本文件移入 `docs/done/`。
  - **README 不更新** —— TG 心跳是已有桌面心跳的多端补强，与 cancel/retry/tasks/help 系列同性质。
  - **设计取舍**：与桌面心跳共享 threshold（同一 settings 字段，多通道无理由用不同阈值）；去重粒度 `(title, updated_at)`（粗到不会刷屏，细到 LLM 一旦写新进度就重新计时）；终态显式 remove `last_heartbeat[title]`（防 cancel/retry 翻回 pending 时仍有旧 entry 阻塞）；cleanup 步骤 `retain(seen_titles)` 防止删任务后内存泄漏。
  - **未做手动 dev 验证**：当前会话不便启动 Tauri 桌面 app；纯函数有单测，watcher 集成路径与既有终态通知同模板（与 first_pass / 60s 间隔 / TaskOrigin 解析共享）。
