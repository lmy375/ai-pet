# PanelDebug 决策日志加"今日累计"计数（Iter R108）

> 对应需求（来自 docs/TODO.md）：
> PanelDebug 决策日志加"今日累计"计数：filter 行尾 buffer 状态 "N/16" 旁加 "今日 X 次"（按 timestamp 落本地今日 count），让用户感知日级触发频率，与 R86 时间窗 chip 互补（窗口是滑动；今日是日历对齐）。

## 目标

filter 行尾现已显 `{filtered}/{total} · buffer {N}/16`：当前过滤命中数、
ring buffer 总数、buffer 容量状态。但用户审视"宠物今天主动开口频率"时，
得肉眼数 timestamp 行落今日的；R86 时间窗（10/30/60m 滚动）也不对齐"日
历今天"。

加 `今日 X` 计数显在 buffer 状态之后（同 muted 文案行）。tooltip 说明数
据上限（受 ring buffer cap 16 限制；满 buffer 时今日可能 > 16 但只显
buffer 中的）。

## 非目标

- 不动后端 CAPACITY —— 只读 ring buffer，不追历史
- 不持久化 —— 每渲染重算（≤16 廉价）
- 不与 R86 时间窗联动显隐 —— 二者互补：窗口是滑动小尺度；今日是日历对齐
  全天

## 设计

### useMemo

```ts
const todayDecisionCount = useMemo(() => {
  const todayStart = new Date();
  todayStart.setHours(0, 0, 0, 0);
  const todayMs = todayStart.getTime();
  let count = 0;
  for (const d of decisions) {
    const ts = Date.parse(d.timestamp);
    if (!Number.isNaN(ts) && ts >= todayMs) count++;
  }
  return count;
}, [decisions]);
```

放 `filteredDecisions` useMemo 旁边，依赖 `decisions` 全集（不受 filter
影响 —— 今日累计是绝对值不该被 filter 缩小）。

`setHours(0, 0, 0, 0)` 取本地午夜 ms；与 R89 任务"今日完成"统计同款边界
处理保持一致。

### 渲染

在现有 `· buffer N/16` 后追加：

```diff
 <span ...buffer status...>
   · buffer {decisions.length}/16
 </span>
+<span style={{ marginLeft: 4 }}>
+  · 今日 {todayDecisionCount}
+  {decisions.length >= 16 && (
+    <span title="ring buffer 已满 16 条；今日实际触发数可能更多但被淘汰" style={{ marginLeft: 1 }}>
+      +
+    </span>
+  )}
+</span>
```

- 分割符 `·` 与既有"过滤数 · buffer"风格一致
- buffer 满时附 `+` 暗示"今日实际可能更多"，tooltip 解释完整原因

### 颜色

继承父 span 的 `color: var(--pet-color-muted)`，与现有"buffer N/16"同色族。
不引入额外 accent —— 统计性质 muted。

### 测试

无单测；手测：
- 早上 buffer 空 → "今日 0"
- 触发 3 次 → "今日 3 · buffer 3/16"（buffer 还没满）
- 触发 20 次（buffer 容量 16）→ "今日 16+ · buffer 16/16"
- 跨午夜 → 旧 timestamp 不再算"今日"，count 自动 reset
- 切到不同 kind / 时间窗 / reason 过滤 → 今日数不变（不受 filter 影响）

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | useMemo + 渲染 |
| **M2** | tsc + build |

## 复用清单

- 既有 decisions ring buffer
- 既有 buffer status span 容器
- R89 today-window 边界处理（setHours 本地午夜）

## 进度日志

- 2026-05-09 13:00 — 创建本文档；准备 M1。
- 2026-05-09 13:08 — M1 完成。`todayDecisionCount` useMemo（不受 filter 影响，依赖 decisions 全集）setHours(0,0,0,0) 算本地午夜阈值；filter 行尾 buffer 状态后追加 `· 今日 N`；buffer 满 16 时附 `+` 后缀 + tooltip 解释"实际可能更多被淘汰"。
- 2026-05-09 13:11 — M2 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (500 modules, 952ms)。归档至 done。
