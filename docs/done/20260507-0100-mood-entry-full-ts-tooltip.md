# mood entry 列表 hover 显示完整时间戳 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> mood entry 列表 hover 显示完整时间戳：当日详情列表只显示 HH:MM；hover 时 title 显示完整 RFC3339（含日期 + 秒），方便对照其它日志精确时间。

## 目标

drill 当日 mood entries 列表只显 HH:MM，对调 LLM 日志 / butler_history /
其它系统的精确时间时不够。给 HH:MM span 加 `title={entry.timestamp}`，hover
出 tooltip 显示完整 RFC3339（含日期 + 秒 + 时区）。

## 非目标

- 不改 entries 默认显示（HH:MM 在窄列表里足够紧凑）。
- 不导出 markdown 时切换到完整 ts —— 复盘笔记里 HH:MM 已够（与现有 MD
  导出格式 `### date\n- HH:MM` 一致）。

## 设计

只改一个 prop：`<span>` 包 HH:MM 处加 `title={entry.timestamp}`。
原始 RFC3339 已经在 `entry.timestamp` 字段里现成可用。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | title attr + tsc + build |

## 进度日志

- 2026-05-07 01:00 — 创建本文档；准备 M1。
- 2026-05-07 01:05 — M1 完成。HH:MM span 加 `title={entry.timestamp}` 让 hover tooltip 显示完整 RFC3339；`pnpm tsc --noEmit` 0 错误，`pnpm build` 通过。归档至 done。
