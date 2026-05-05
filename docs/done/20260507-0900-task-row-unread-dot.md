# 任务列表行展示"未读"红点 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> 任务列表行展示"未读"红点：列表行仅在展开时才看历史，列表层无 unread 信号；用既有 lastview localStorage 与 task.updated_at 比较，updated_at 更新且大于 lastview → 行尾显红点，提示有新事件待看。

## 目标

上一轮加了任务详情时间线的 🆕 已读判定，但用户必须**先展开**才看得到。
本轮把 unread 信号上提到列表行：使用同一份 `pet-task-history-lastview-{title}`
localStorage，和 `task.updated_at` 比对；**有 prior view + updated 比 prior 新**
→ 行尾显小红点，让用户在列表层就看到"哪些任务有新动静要看"。

## 非目标

- "从未打开过"的任务**不**显红点 —— 否则刚装机时所有任务都会带点，噪
  音淹没真信号；既有"刚动过"绿点已能覆盖"全新任务" / "刚被改" 场景。
- 不影响排序 —— 红点纯视觉，不动列表顺序（用户的排序偏好优先）。
- 不与"刚动过"绿点冲突 —— 两点语义不同：绿 = "5 分钟内动过"（绝对
  时钟），红 = "我看过之后又动了"（相对个人 view）。同一行可同时显两点。

## 修正既有 bug 顺手做

前一轮的「已读」判定用了 `ev.timestamp > prev` lex 比较。但 prev 写入用
`new Date().toISOString()` 是 UTC Z 格式（`...Z`），而 backend 发的 ts 是
本地时区格式（`...+08:00`），lex 比较会错（'+' 与 'Z' 字符顺序错位）。

本轮抽出一个 `tsAfter(a, b)` 纯辅助函数：用 `Date.parse` 把两边转 ms 比
较，正确处理跨时区表达式。在 history timeline 与新增的 row dot 都走这
个函数，避免两边各自踩坑。

## 设计

### 纯函数

```ts
/** 比较两个 RFC3339 / ISO8601 字符串：a 时刻晚于 b 才返回 true。
 * b === null → true（"无 prior 都视作新"，调用方决定是否需要这语义）。
 * 任一解析失败（理论不会发生） → false。 */
function tsAfter(a: string, b: string | null): boolean {
  if (b === null) return true;
  const at = Date.parse(a);
  const bt = Date.parse(b);
  if (Number.isNaN(at) || Number.isNaN(bt)) return false;
  return at > bt;
}
```

放在文件顶部 utils 区，同 `isDueToday` / `isOverdue` 一起。

### unread row dot

每个 task 行渲染时调：
```ts
const lastview = localStorage.getItem(`pet-task-history-lastview-${t.title}`);
const isUnread = lastview !== null && tsAfter(t.updated_at, lastview);
```

`lastview === null` → **不**显红点（"从未打开"是绿点的事）。

视觉：在既有绿点旁加一个小红点 (`#dc2626`)，title hover 解释 "上次打开此
任务后又有了更新"。

### 实时性

`lastview` 不是 React state；展开任务时写 localStorage 不会触发 list 行
re-render。但 PanelTasks 内 `nowMs` state 每 30s 刷一次（既有"刚动过"
绿点机制），下次 nowMs tick 时 row 自然 re-render，红点会按 lastview
新值消失。30s 滞后对"刚展开 → 红点立即消失"的瞬时反馈不算理想。

为了即时反馈，加一个 `lastviewBump: number` state，handleToggleExpand 写
完 localStorage 后 setLastviewBump(n => n+1)，触发列表 re-render。计算红
点时不直接用 bump 值（仅作为 reactivity 触发器）。

### 既有 history 判定补丁

把 `ev.timestamp > prev` 替换为 `tsAfter(ev.timestamp, prev)`：修跨时区
lex 比较 bug。同时 `prev === null` 仍走 "全部视为新" 路径（与 tsAfter
的 null 行为一致）。

## 测试

PanelTasks 容器 IO 重；前端无 vitest。`tsAfter` 是 4 行纯函数，复杂度 0；
跨时区比较通过手测验证。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | tsAfter 纯函数 + 替换既有 history 判定 |
| **M2** | lastviewBump state + handleToggleExpand 内 setLastviewBump |
| **M3** | 行渲染加红点（仅当 lastview !== null && tsAfter(updated_at, lastview)） |
| **M4** | tsc + build + cleanup |

## 复用清单

- 既有 `pet-task-history-lastview-{title}` localStorage key
- 既有 isRecentlyUpdated 绿点旁的视觉位
- 既有 nowMs 30s 刷新（保底）

## 进度日志

- 2026-05-07 09:00 — 创建本文档；准备 M1。
- 2026-05-07 09:10 — M1 完成。`tsAfter(a, b)` 纯函数加在 isOverdue 旁；用 Date.parse 转 ms 比较跨时区表达式（修了上一轮 history 用 lex `>` 比较 UTC Z vs Local +08:00 的 latent bug）；history map 内 `tsAfter(ev.timestamp, prev)` 替换原 `> prev`。
- 2026-05-07 09:15 — M2 完成。`lastviewBump` state 加在 lastViewRef 旁；handleToggleExpand 写完 localStorage 后 setLastviewBump(n+1) 触发列表 re-render。
- 2026-05-07 09:25 — M3 完成。任务行绿点旁 IIFE 渲染红点：localStorage.getItem(key) → null 不显（避免初装满屏）→ 用 tsAfter(t.updated_at, lv) 判断；try/catch 兜底；title hover 解释 "距上次展开此任务后又有更新 — 点击展开看新事件"。
- 2026-05-07 09:30 — M4 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (498 modules, 929ms)。归档至 done。
