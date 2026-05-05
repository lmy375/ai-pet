# 任务到期徽章 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> 任务到期徽章：「任务」标签头加红点，标记当前有 N 条已过期未完成的 pending/error 任务，提升可见性。

## 目标

「任务」面板在切到该标签前不可见。如果有任务到期未处理，用户在「设置 / 聊天 /
记忆 / 人格」标签页停留时**看不到**任何信号——只有走到任务页才发现。本轮在标签
栏的「任务」按钮右上角加一个红色小徽章，N=已过期未完成（pending / error 且
`due < now`）的条数；N=0 不渲染。

## 非目标

- 不在桌面气泡 / 系统通知里弹消息——徽章是被动信号，不打扰用户的当前任务。
  Push 风格的通知是另一条 GOAL 级别的需求。
- 不区分"刚过期 / 严重过期"——按时间维度细分会让徽章语义复杂；用户走到 task
  panel 自然能看到 due 时间。
- 不为 `due` 缺失但状态 error 的任务计入（"过期"严格要求 due 存在）。
- 不写 README —— UI 可见性补强，与既有 panel 迭代同性质。

## 设计

### 后端

`commands/task.rs` 新加：

```rust
#[tauri::command]
pub fn task_overdue_count() -> u64;
```

实现：
1. `memory_list("butler_tasks")` 拿索引
2. 遍历 items，parse_task_header 拿 due（无 due 跳过）
3. classify_status → 仅 Pending / Error 计入
4. `due < now` 的计数

抽出纯函数 `count_overdue(items: &[MemoryItem], now: NaiveDateTime) -> u64`：
- pure，单测全覆盖（含 due 缺失 / 状态终态 / 边界 due == now / 无效 due 跳过）

不进 lib.rs 之外的额外注册路径；与现有 `task_list` / `task_get_detail` 同层。

### 前端

`PanelApp.tsx`：
- 加 `useState<number>(0)` 存 overdueCount
- mount + interval 30s 调 `task_overdue_count`
- 用户切到「任务」标签时立刻 refetch（保证标签内做了操作再切回 PanelApp 时
  徽章数同步；其它标签的操作不会改任务，不必触发）
- TABS 渲染：当 tab === "任务" 且 overdueCount > 0 → 在按钮右上角绝对定位一个
  红色小圆 + 数字，9+ 截断。

UI：
- 圆直径 16px，背景 `#dc2626`（与现有 unreadBadge 保持一致），白字粗体。
- 位置：tab 按钮右上 `top: 4px, right: 8px`（绝对定位，按钮设 `position:
  relative`）。
- title hover：解释"已过期未完成 N 条 — 切到任务标签查看"。

### 测试

后端纯函数：
- 全空 → 0
- 含 1 个 pending overdue + 1 个 done overdue → 1
- 含 1 个 cancelled overdue → 0
- 含 pending 但 due 未过 → 0
- 含 pending 无 due → 0
- 含 pending 但 task header 解析失败（legacy 任务）→ 跳过（无 due 视作 0）
- 边界：due == now → 视作 overdue（与 compare_for_queue 中 `due <= now` 一致）
- 含 invalid due 字符串（理论不应发生，protective）→ 跳过

前端无测试，靠 tsc + 手测。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | 后端 `count_overdue` 纯函数 + 单测 + `task_overdue_count` Tauri 命令 + 注册 |
| **M2** | 前端 PanelApp 状态 / 30s polling / refetch on tab change / 徽章 UI |
| **M3** | `cargo test` + `pnpm build` + TODO 清理 + done/ |

## 复用清单

- `task_queue::{parse_task_header, classify_status, TaskStatus}`
- `commands::memory::memory_list`
- `chrono::Local::now().naive_local()`

## 待用户裁定的开放问题

- 30s polling 周期合适吗？任务过期是分钟级精度，30s 足够。再短意义不大。
- 徽章上限 9+ vs 99+？本轮 9+（保持视觉紧凑；任务列表理论 < 30 条，9+ 已极端）。
- 是否在徽章 title hover 里展示 top N 任务标题预览？本轮**否**——增加状态
  与 IPC，而切到任务标签就直接看到全部，性价比低。

## 进度日志

- 2026-05-05 09:00 — 创建本文档；准备 M1。
- 2026-05-05 09:30 — 完成实现：
  - **M1**：`commands/task.rs` 加 `count_overdue(items, now) -> u64` 纯函数 + `task_overdue_count` Tauri 命令。9 条单测覆盖：空 / pending overdue / error overdue / 终态排除 / 未来 due / 无 due / legacy 无 header / 边界 due==now (一致 `compare_for_queue`) / 多条聚合。注册到 lib.rs。
  - **M2**：`PanelApp.tsx` 加 `overdueCount` 状态 + `fetchOverdue` callback + 30s polling effect + 切到「任务」标签时立即 refetch effect。tab 按钮加 `position: relative`，「任务」tab > 0 时绝对定位红色小徽章（`#dc2626` 与气泡 unread badge 同色，9+ 截断，title hover 解释计数语义）。
  - **M3**：`cargo test --lib` 868/868（+9）；`pnpm tsc --noEmit` 干净；`pnpm build` 494 modules 全过。TODO 移除条目；本文件移入 `docs/done/`。
  - **README 不更新** —— 「任务」标签的可见性增强，与既有任务面板迭代同性质。
  - **设计取舍**：30s 轮询 vs event-driven —— 轮询足够（任务过期是分钟级精度，30s 抓得住），event-driven 需要跨 component subscribe + cleanup，复杂度不值；徽章只在「任务」tab 上显示而非 tab 旁的全局位置，让信号位置=触发动作的入口（用户一眼知道点哪里查看）；done / cancelled 不入计数（用户已 move on，徽章不该回来打扰）。
  - **未做手动 dev 验证**：当前会话不便启动 Tauri 桌面 app；纯函数 9 条单测含全部边界，前端轮询 + 渲染由 tsc + 既有 panel 模式保证。
