# 任务历史时间线"已读"标记 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> 任务历史时间线"已读"标记：detail 面板时间线条目都显示同色，分不清"我看过的"和"新发生的"；用 localStorage 记录每个任务最后查看时刻，比 timestamp 新的事件加 "🆕" 标记。

## 目标

PanelTasks 任务详情的「事件时间线」按时间倒序列 butler_history 事件。
当任务跑得久（多轮 update / retry / cancel），用户回去检查时无法快速
分辨"上次我看到的进度"和"这之后新发生的"。本轮用 localStorage 记录每
个 task title 的"上次查看时刻"，比那时刻新的 history 条目前缀显 🆕。

## 非目标

- 不持久 mark-as-read 到后端 —— 已读状态是用户阅读偏好，与 butler_history
  本身无关；后端不该承担这个语义。
- 不做"全部标已读" / 手动标记入口 —— 自动按"上次展开本任务"判定足够；
  显式按钮反而让用户操心 mark 流程。
- 不影响任务列表排序 / 提示 —— 排序 axis 已多（priority / due / status）；
  把"任务有新事件"提到列表行级是另一件事，先做面板内 inline 标记。

## 设计

### 存储

`localStorage` key：`pet-task-history-lastview-{title}`，值为 RFC3339
本地时间字符串。

为何按 title 而非 task id：butler_tasks 没有稳定 id —— title 是 yaml 主
键 + 重命名极少（重命名会被 memory_edit 当 create 处理）。同名任务（用户
两次创建同名）会共享 lastview，权衡为可接受 —— 罕见 + 错误模式只是误标
"看过"，无破坏性。

### 触发：何时更新 lastview

在 `handleToggleExpand`（用户主动展开任务详情）内：
- 折叠分支（再次点同一任务）→ 不动（防止"刚展开看完就被自动标已读"）
- 展开分支：
  1. 读 `prev = localStorage.getItem(key)` 存到组件 ref Map（让 render
     用旧值判断 🆕，新值已写到 localStorage 不影响本次 render）
  2. 写 `localStorage.setItem(key, nowRFC3339)` 

这样**首次展开**时所有事件都对照"无 prev = null"，全部显 🆕（合理 —
初次看 = 全是新的）。后续展开按"上次展开时刻"对比。

### 渲染

每个 history 事件根据 ref Map 拿 prev：
```ts
const prev = lastViewRefMap.get(title) ?? null;
const isNew = prev === null || ev.timestamp > prev;
```

ts 比较用字符串 lex —— RFC3339 lex 序与时间序一致（带固定时区或同时区
时），后端写入用 chrono::Local，前端 lastview 也写同源 Local，比较正确。

🆕 在 action icon 之前显，不抢"action 类型"主信息位。

### 实现细节

`lastViewRef` 用 `useRef<Map<string, string | null>>(new Map())`，避免
state setter 触发 re-render（值在 toggle 时一次性写入，render 只读）。

不在卸载 / 折叠时清条目 —— Map 与组件生命周期同源，PanelTasks 重 mount
时（切 tab + 切回）重新读 localStorage 是符合预期的。

## 测试

PanelTasks 是 IO 重容器；前端无 vitest。localStorage 操作通过手测覆盖：
- 第一次展开任务 → 全部条目带 🆕
- 折叠 → 重新展开 → 之前的条目无 🆕
- 触发新事件（让 LLM 跑一轮）→ reload → 展开 → 仅新事件带 🆕

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | lastViewRef Map + handleToggleExpand 内 read-then-write |
| **M2** | history.map 内根据 prev 加 🆕 前缀 |
| **M3** | tsc + build + cleanup |

## 复用清单

- 既有 `handleToggleExpand` callback
- 既有 history 渲染循环

## 进度日志

- 2026-05-07 08:00 — 创建本文档；准备 M1。
- 2026-05-07 08:10 — M1 完成。`lastViewRef = useRef<Map<string, string|null>>` 缓存 read-then-write 拿到的旧值；handleToggleExpand 展开分支前置 read prev → write nowIso 到 localStorage 的 `pet-task-history-lastview-${title}` 键；try/catch 保 localStorage 失败不阻断展开。
- 2026-05-07 08:15 — M2 完成。history.map 外层 IIFE 取 prev；逐条比较 `ev.timestamp > prev`（RFC3339 lex 序与时间序一致）；新事件加红字 🆕 前缀，紧跟 ts 之后。
- 2026-05-07 08:20 — M3 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (498 modules, 951ms)。归档至 done。
