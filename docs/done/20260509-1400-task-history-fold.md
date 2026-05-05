# PanelTasks 任务详情 history timeline 折叠（Iter R109）

> 对应需求（来自 docs/TODO.md）：
> PanelTasks 任务详情 history timeline 折叠：detail.history 列表 > 8 条时默认显前 5 条 + "展开全部 N 条"按钮（仿 R91 / R95 折叠模式）。长跑任务的 history 可累积几十条 update / delete 行，挤压 detail 视觉空间。

## 目标

PanelTasks 详情面板的"事件时间线"段直接 dump `detail.history` 全集。长跑任
务（butler 周期任务、定时提醒等）会累积几十条 update / delete / create 事
件，挤压 detail 视觉空间，把 detail.md / 描述等其它段推到看不见。

加默认折叠：> 8 条时只显**最新** 5 条，列表顶部加"展开更早 N 条"按钮。

## 后端 history 排序方向核实

`detail.history` 在前端是按时间序展示，🆕 标记逻辑（line 2751）`tsAfter
(ev.timestamp, lastView)` 只对"比 lastView 新"的事件打标 —— 用户体验是
"最新事件在底部"。折叠取最后 5 条 = 最新 5 条，符合"先看最近发生了什么"。

## 非目标

- 不持久化展开状态 —— session 内有效，重开 detail 自动复位（与 R91 / R95
  / R102 同语义）
- 不动事件渲染 / 🆕 标记 / time format —— 只在 .map 输入上做切片
- 不引入 reverse 反转默认序 —— "时间正序，最新在底"是当前 UX 共识

## 设计

### state

```ts
const [expandedHistoryTitles, setExpandedHistoryTitles] = useState<Set<string>>(
  new Set(),
);
```

key 用任务 title（与 `expandedTitle` / `selected` Set 等其它 per-task state
同模式）。Set 风格让多个 task 折叠状态独立（虽然实际上同时只一个 task
detail 展开，但保持模式统一不会出错）。

### 折叠规则

```ts
const HISTORY_FOLD_THRESHOLD = 8;
const HISTORY_FOLD_PREVIEW = 5;
const isLongHistory = detail.history.length > HISTORY_FOLD_THRESHOLD;
const historyExpanded = expandedHistoryTitles.has(t.title);
const displayedHistory =
  isLongHistory && !historyExpanded
    ? detail.history.slice(-HISTORY_FOLD_PREVIEW)
    : detail.history;
```

`slice(-5)` 取最后 5 条（最新 5 条）。≤ 8 条不折叠（与 R91 / R102 阈值
+5 缓冲一致 —— 临界值 6/7/8 不强制折叠避免引入"折出来的 4 条 vs 全 7 条"
无意义切换）。

### 渲染

在 `<div style={s.historyList}>` 内、map 之前加按钮：

```tsx
{isLongHistory && (
  <button
    type="button"
    onClick={() =>
      setExpandedHistoryTitles((prev) => {
        const next = new Set(prev);
        if (next.has(t.title)) next.delete(t.title);
        else next.add(t.title);
        return next;
      })
    }
    title={
      historyExpanded
        ? `折叠回最新 ${HISTORY_FOLD_PREVIEW} 条`
        : `展开后显示全部 ${detail.history.length} 条`
    }
    style={{
      marginBottom: 4,
      fontSize: 11,
      padding: 0,
      border: "none",
      background: "transparent",
      color: "var(--pet-color-accent)",
      cursor: "pointer",
      fontFamily: "inherit",
    }}
  >
    {historyExpanded
      ? `收起 (${detail.history.length})`
      : `… 展开更早 ${detail.history.length - HISTORY_FOLD_PREVIEW} 条`}
  </button>
)}
{displayedHistory.map((ev) => ...)}
```

inline 链接式 accent 按钮（与 R91 / R95 / R102 同款样式）。"展开更早 N
条"文案 specifically 提示展开后会看到的是更早的事件（vs"展开全部"含糊）。

### 测试

无单测；手测：
- detail.history.length === 8：不折叠（≤ 阈值），无按钮
- detail.history.length === 12：默认显最后 5 条 + "展开更早 7 条"按钮在顶
- 点展开 → 显全部 12 条 + "收起 (12)"
- 切到不同 task 详情 → 该 task 的折叠状态独立（Set per title）
- 关闭再开同一 task：保留展开状态（Set 不被 clear）

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | state + 折叠规则 + 按钮 + 切换 displayedHistory |
| **M2** | tsc + build |

## 复用清单

- 既有 R91 / R95 / R102 折叠按钮 inline accent 样式
- 既有 detail.history 数据 / 渲染路径

## 进度日志

- 2026-05-09 14:00 — 创建本文档；准备 M1。
- 2026-05-09 14:08 — M1 完成。`expandedHistoryTitles: Set<string>` state；history IIFE 内常量 THRESHOLD=8 / PREVIEW=5；slice(-5) 取最新 5；isLong + expanded 决定 displayedHistory；button 在 map 之前位（顶部）渲染 inline accent 按钮，文案 "展开更早 N 条" / "收起 (N)"，提示语义"展开后看到的是更早事件"。
- 2026-05-09 14:11 — M2 完成。`pnpm tsc --noEmit` 0 错误。归档至 done。
