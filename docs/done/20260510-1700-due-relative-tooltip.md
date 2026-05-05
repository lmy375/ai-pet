# PanelTasks due chip hover 距今天数（Iter R136）

> 对应需求（来自 docs/TODO.md）：
> PanelTasks due chip hover 显距今天数：现 due chip 显 "截止 MM-DD HH:MM" + dueUrgency tooltip "已过期 / 24 小时内"；补精确数字 "X 天后到期" / "已过 X 天"，让用户快速判断紧迫度（仿 R87 任务 created_at 相对时间风格）。

## 目标

PanelTasks 任务卡 due chip 现 tooltip 在 due 距今 < 24h 时显 "24 小时内
到期"、过期显 "已过期"、否则无。但用户经常想知道：
- 还有 3 天 / 5 天 / 1 周 到期？
- 已过期多久？

R87 给 created_at 加了相对时间附文 "X 天前创建"。本轮镜像到 due chip
hover tooltip：精确数字让用户快速判断紧迫度。

## 非目标

- 不动 chip 文字本身（仍 "截止 MM-DD HH:MM"）—— 改文案会破坏对齐 / 视觉
  排序习惯
- 不区分小时 / 分钟级 —— 1 天内统一 "X 小时后" 或 "X 小时前"；< 1 小时
  归 "1 小时内"。粒度足够紧迫感判断
- 不给非-due 任务任何 tooltip —— due === null 时 chip 不渲染（既有逻辑）

## 设计

### relative formatter

```ts
function formatDueRelative(dueIso: string, now: number): string {
  const ts = Date.parse(dueIso);
  if (Number.isNaN(ts)) return "";
  const diffMs = ts - now;
  const absMs = Math.abs(diffMs);
  const days = Math.floor(absMs / 86_400_000);
  const hours = Math.floor(absMs / 3_600_000);
  const future = diffMs >= 0;
  if (absMs < 3_600_000) {
    return future ? "1 小时内到期" : "刚过期";
  }
  if (days < 1) {
    return future ? `${hours} 小时后到期` : `已过 ${hours} 小时`;
  }
  return future ? `${days} 天后到期` : `已过 ${days} 天`;
}
```

边界处理：
- < 1 小时（无论先后）→ "1 小时内到期" / "刚过期"
- < 1 天（≥ 1 小时）→ "X 小时后" / "已过 X 小时"
- ≥ 1 天 → "X 天后" / "已过 X 天"

`now` 由 `nowMs` 提供（已存在每 30s 自动 tick）。

### 渲染

把现 tooltip 的 enum-like 文案附加 relative：

```diff
 const urgency = dueUrgency(t.due, nowMs, t.status);
-const tooltip =
-  urgency === "overdue"
-    ? "已过期"
-    : urgency === "soon"
-      ? "24 小时内到期"
-      : undefined;
+const relative = formatDueRelative(t.due, nowMs);
+const tooltip =
+  urgency === "overdue"
+    ? `已过期：${relative}`
+    : urgency === "soon"
+      ? `24 小时内到期：${relative}`
+      : relative; // urgency === "normal" 也显，让用户随时知道距今多久
```

normal urgency 也显 relative —— 之前 tooltip undefined（不显 hover 提示）。
现有提示让用户随时点 chip 知精确距离。

### 测试

无单测；手测：
- due = 30 分钟后 → tooltip "24 小时内到期：1 小时内到期"（叠加显示，"24
  小时内"是 enum，"1 小时内到期"是 relative）
- due = 5 小时后 → "24 小时内到期：5 小时后到期"
- due = 3 天后 → "3 天后到期"（normal urgency，无 enum 前缀）
- due = 30 分钟前 → "已过期：刚过期"
- due = 5 小时前 → "已过期：已过 5 小时"
- due = 3 天前 → "已过期：已过 3 天"
- 跨午夜 30s 后 → relative 自动滚动到下一档

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | formatDueRelative helper |
| **M2** | tooltip 文案叠加 relative |
| **M3** | tsc + build |

## 复用清单

- 既有 `nowMs` state（30s tick）
- R87 / R89 / R92 同款相对时间分级
- 既有 dueUrgency / formatDue

## 进度日志

- 2026-05-10 17:00 — 创建本文档；准备 M1。
- 2026-05-10 17:08 — M1 完成。`formatDueRelative` helper 加在 formatRelativeAge 之下（同款分级逻辑，但是双向区分 future / past + 三档：< 1h "1 小时内 / 刚过期"、< 1d "X 小时后 / 已过 X 小时"、≥ 1d "X 天后 / 已过 X 天"）。
- 2026-05-10 17:11 — M2 完成。due chip IIFE 内 tooltip 改：overdue → `已过期：${relative}`；soon → `24 小时内到期：${relative}`；normal → 直接 relative（之前 undefined 不显，现在统一）。
- 2026-05-10 17:14 — M3 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (500 modules, 974ms)。归档至 done。
