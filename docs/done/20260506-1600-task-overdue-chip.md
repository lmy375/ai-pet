# 任务面板逾期 chip — 开发计划

> 对应需求（来自 docs/TODO.md）：
> 任务面板逾期 chip：与「今日到期」并列，加 "逾期 N"（due 已过 & 未结束）一键过滤；逾期是比"今天到期"更紧迫的状态，目前要靠红色 due 字色肉眼挑。

## 目标

PanelTasks 已有「今日到期」chip，但更紧迫的「逾期」（due 已过且未结束）
仍只靠红色 due 字色肉眼区分。本轮加一个 "🔴 逾期 N" chip，与「今日到期」
**互斥并列**：用户在 due 维度上始终只看一类（全部 / 今日 / 逾期），不必
心算两者交集。

## 非目标

- 不做"7 天内到期" / "本周到期" —— 档次太多反而让选择变难。
- 不做 sortMode 联动（开"逾期"自动按 due 排序）—— 用户应仍能选 queue
  排序看 priority 顺序。
- 不动既有红色 due 字色 —— 那是行级紧迫提示，与"批量过滤"是不同 axis。

## 设计

### state 重构（最小改动）

`dueTodayOnly: boolean` → `dueFilter: "all" | "today" | "overdue"`。三态 enum
比"两个独立 boolean + 互斥逻辑"更直观，避免"两个都开"的死状态。

### 新增纯函数

`isOverdue(due, now, status)` 复用既有 `dueUrgency`：
```ts
function isOverdue(
  due: string | null,
  now: number,
  status: TaskStatus,
): boolean {
  return due !== null && dueUrgency(due, now, status) === "overdue";
}
```

`dueUrgency` 已经处理：
- 终态（done/cancelled）一律 "normal" → 永远不会被判逾期
- 解析失败 → "normal" → 同上
- 未来时间 → "soon" / "normal"
- 过去时间 → "overdue"

不需要新写日期解析。

### 过滤合成

`filteredTasks` 现有 dueTodayOnly 段：
```ts
.filter((t) => {
  if (!dueTodayOnly) return true;
  return !isFinished(t.status) && isDueToday(t.due, nowDate);
})
```

改为：
```ts
.filter((t) => {
  if (dueFilter === "all") return true;
  if (dueFilter === "today") {
    return !isFinished(t.status) && isDueToday(t.due, nowDate);
  }
  // dueFilter === "overdue"
  return isOverdue(t.due, nowMs, t.status);
})
```

注意 `nowDate` 是 Date / `nowMs` 是 number，dueUrgency 用 `Date.parse` 拿
ms。再起一个变量名就好。

### 计数

新增 `overdueCount` useMemo，与 `dueTodayCount` 同源派生 tasks 全集。

### UI

把现有"今日到期"chip 那一行改成 chip 组（横向 wrap）：
- "今日到期 N"（橙）
- "逾期 N"（红）

点击一个 chip：
- 当前激活的同 chip → 切回 "all"（=关闭过滤）
- 其它 → 切到该 chip

仅当至少一个计数 > 0 时整行渲染。每个 chip 自己计数为 0 时不渲染（避免
"逾期 0"长期占位）。

颜色：
- 今日到期：橙 `#fed7aa` bg / `#9a3412` fg（既有）
- 逾期：红 `#fecaca` bg / `#991b1b` fg（与 dueColor 的 overdue 红 `#dc2626`
  同色系但稍浅，让 chip 视觉重而不刺）

### filtersActive

`dueFilter !== "all"` 视为 active。

## 测试

`isOverdue` 是 `dueUrgency === "overdue"` 的薄包装；前端无 vitest。靠 tsc
+ 手测 —— sparkline / 任务面板都已有大量 tsc 类型检查兜底回归。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | dueTodayOnly → dueFilter 重命名 + isOverdue 函数 |
| **M2** | filteredTasks / filtersActive 接 dueFilter |
| **M3** | overdueCount + chip 组 UI |
| **M4** | tsc + build + cleanup |

## 复用清单

- 既有 `isDueToday` / `dueUrgency` / `isFinished`
- 既有 `dueTodayCount` 计算模式（派生自 tasks 全集）
- 既有 chip 视觉容器 (`s.tagFilterRow` style)

## 进度日志

- 2026-05-06 16:00 — 创建本文档；准备 M1。
- 2026-05-06 16:10 — M1 完成。`isOverdue(due, now, status)` 加在 `isDueToday` 旁；`dueTodayOnly: boolean` 重构为 `dueFilter: "all" | "today" | "overdue"` 三态 enum。
- 2026-05-06 16:20 — M2 完成。`filteredTasks` 链改 enum 分支；`overdueCount` + `dueTodayCount` 合并到一个 useMemo，依赖 nowMs 让 30s 时钟驱动重算；filtersActive 跟 dueFilter !== "all"。
- 2026-05-06 16:25 — M3 完成。新增 `DueChip` 组件复用同一行：逾期红 `#fef2f2 / #991b1b`、今日橙（既有色），互斥由父级 dueFilter 保证。逾期 chip 在前（更紧迫），今日在后。
- 2026-05-06 16:30 — M4 完成。`pnpm tsc --noEmit` 0 错误（修一处 nowMs 重声明：复用既有"最近更新"绿点的 nowMs 状态，让两套时钟同步）；`pnpm build` 通过 (498 modules, 936ms)。归档至 done。
