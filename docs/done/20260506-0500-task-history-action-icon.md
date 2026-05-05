# 任务详情时间线 action 图标 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> 任务详情时间线 action 颜色映射展示：current `historyAction(action)` 已支持 create/update/delete 三色；考虑也展示 action 缩写图标（➕✎🗑）让一眼能扫，同时图标 + 文字双通道。

## 目标

任务详情面板里的事件时间线行 `<span style={s.historyAction}>{ev.action}</span>`
当前只用颜色 + 字面英文（create / update / delete）区分动作。本轮在 action
徽章里前置一个小图标，让色彩 + 图标 + 文字三通道并行，扫读速度更快。

## 非目标

- 不动 backend 的 action 字符串契约（仍是 `create` / `update` / `delete`）。
- 不为未来的新 action（理论上不会有）预留通配；落到 default 分支的 `•` 占位。
- 不写 README —— 任务详情视觉微调。

## 设计

- 加 file-level pure helper `actionIcon(action: string): string`：
  - `create` → `➕`
  - `update` → `✎`
  - `delete` → `🗑`
  - default → `•`
- JSX 改成 `<span ...>{actionIcon(action)} {action}</span>` —— 图标 + 半角空格
  + 字面英文，紧凑且键盘屏幕阅读器友好（仍能读出 "create"）。
- `historyAction(action)` 颜色函数不动，前缀 emoji 与现有徽章配色互补。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | actionIcon helper + JSX 接入 |
| **M2** | tsc + build + cleanup |

## 测试

逻辑全 pure 4 行；无 vitest。靠 tsc + 手测。

## 复用清单

- 既有 `s.historyAction(action)` 颜色样式

## 待用户裁定的开放问题

- 选 `✎` vs `📝` for update？前者是"铅笔"字符 (U+270E)，更紧凑；后者 emoji
  风格更突出但与 `➕` `🗑` 都是 emoji 样式更协调。本轮选**对齐 emoji 风格**：
  改成 `📝`。
- create 选 `➕` vs `🆕`？`➕` 更通用，`🆕` 太花哨。本轮 `➕`。

## 进度日志

- 2026-05-06 05:00 — 创建本文档；准备 M1。
- 2026-05-06 05:05 — 完成实现：
  - **M1**：`PanelTasks.tsx` 加 file-level pure helper `actionIcon(action)`：create → ➕ / update → 📝 / delete → 🗑 / default → •。事件时间线 JSX 改为 `<span ...>{actionIcon(ev.action)} {ev.action}</span>`，图标 + 文字双通道（屏幕阅读器仍能读出 action 字面英文）。
  - **M2**：`pnpm tsc --noEmit` 干净；`pnpm build` 497 modules 全过。TODO 移除条目；本文件移入 `docs/done/`。
  - **README 不更新** —— 任务详情视觉微调。
  - **未做手动 dev 验证**：当前会话不便启动 Tauri 桌面 app；helper 4 行 + 一次 JSX 改，由 tsc 保证。
