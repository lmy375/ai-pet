# PanelTasks 剩余 error token 化（Iter R153）

> 对应需求（来自 docs/TODO.md）：
> PanelTasks status / errorMsg / detail 错误剩余迁 token：R152 收尾后
> line 62 / 142 / 1430 / 1468 / 2835 仍 hardcoded `#dc2626`/`#b91c1c`/`#fee2e2`/
> `#fef2f2`，迁到 `tint-orange` family 与 R152 已迁部分一致。

## 目标

R152 把 6 处枚举 line 迁了；这是同文件内剩余的"错误语义" hardcoded 红色：

| line | 用途 | from | to |
| --- | --- | --- | --- |
| 62 | STATUS_BADGE.error bg | `#fee2e2` | tint-orange-bg |
| 62 | STATUS_BADGE.error fg | `#b91c1c` | tint-orange-fg |
| 142 | dueColor("overdue") | `#dc2626` | tint-orange-fg |
| 1430 | errorMsg.color | `#b91c1c` | tint-orange-fg |
| 1468 | actionBtnDanger.color | `#b91c1c` | tint-orange-fg |
| 2835 | editDetailErr inline | `#b91c1c` | tint-orange-fg |

## 非目标

- 不动 dueColor("soon") `#ea580c` —— 是另一档暖色（"即将到期"，不是 error），
  如果 dark 下读不清是单独问题，下轮 TODO。
- 不动 actionBtnDanger 的 `border: "1px solid #fecaca"` / `background:
  "#fff"`：仅按 TODO 把 color 迁。如果未来要彻底 token 化整个按钮，下轮处理。
- 不动 STATUS_BADGE 的 pending/done/cancelled（不是 error 语义）。
- 不动 line 219 PanelOverdueBadge palette（这是 chip badge 自己的 palette
  函数，与 status badge / errorMsg 无共享，独立改）。

## 设计

### 视觉保真

light：`tint-orange-bg`/`fg` = `#fff7ed`/`#9a3412`，与原红 50/700 视觉
临近，警示语义保留。

dark：暗暖底 `#2b1f10` + 亮橙文字 `#fdba74`，与 R147~R152 同框 dark 下警示
统一。

### 替换策略

5 处 `#b91c1c` / 1 处 `#dc2626` / 1 处 `#fee2e2` —— `#b91c1c` 与 `#dc2626`
在文件内可能还在非错误位置（如 STATUS_BADGE 不在本轮 scope），逐 Edit 不
replace_all。

具体 `#fee2e2` 文件内 grep 仅 1 处 (line 62)，可 replace_all 也不冲突。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | 6 处 Edit |
| **M2** | tsc + build |

## 复用清单

- iter 7 tints
- R147 / R150 / R151 / R152 "orange = 警示"约定

## 进度日志

- 2026-05-11 10:00 — 创建本文档；准备 M1。
- 2026-05-11 10:20 — M1 完成：5 处 Edit 全迁 tint-orange；M2 tsc + build
  通过。归档。
