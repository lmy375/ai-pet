# PanelTasks 优先级多选 chip 过滤（Iter R104）

> 对应需求（来自 docs/TODO.md）：
> PanelTasks 优先级多选 chip 过滤：现 p0Only 只有"只看 P0"开关；扩展到 0-9 多选 chip（与决策日志 R83 多选模式一致），让用户组合 "P0 + P3" 任意子集。

## 目标

PanelTasks 当前 priority 过滤是单 bool `p0Only`，只能显 P0 / 不限。但多
priority 任务一起跑的用户经常想看 "P0 + P3"（idea + 中等优先级混合复盘）
或 "P5 以上"（高优集中处理），单 bool 兜不住。

升级到 `Set<number>` 多选：每个出现过的 priority 一个 chip；多选 OR 命中
（任一进集合即通过）；空 Set = 全部。与决策日志 R83 / 工具历史 R39 等
多选 chip 模式一致。

## 非目标

- 不固定渲染 0-9 全部 10 个 chip —— 只显示当前活动任务里出现过的 priority
  （与 dueTodayCount > 0 / overdueCount > 0 才出 chip 一致）
- 不持久化 —— 会话内有效（与现有 dueFilter / search / sort 一致）
- 不重命名 P0 的 "💡 idea drawer" 语义 —— 保留 P0 chip 的灯泡 glyph，让
  老用户继续用直觉

## 设计

### state 替换

```diff
-const [p0Only, setP0Only] = useState(false);
+const [priorityFilter, setPriorityFilter] = useState<Set<number>>(new Set());
```

### counts 派生（替换 p0Count）

```ts
const priorityCounts = useMemo(() => {
  const m = new Map<number, number>();
  for (const t of tasks) {
    if (isFinished(t.status)) continue;
    m.set(t.priority, (m.get(t.priority) ?? 0) + 1);
  }
  // priority asc 序，让 chip 行从 P0 → P9 自然
  return [...m.entries()].sort((a, b) => a[0] - b[0]);
}, [tasks]);
```

### filter 替换

```diff
-.filter((t) => !p0Only || t.priority === 0)
+.filter((t) =>
+  priorityFilter.size === 0 || priorityFilter.has(t.priority),
+)
```

### filtersActive 更新

```diff
-p0Only;
+priorityFilter.size > 0;
```

### clear-all-filters 调整

`setP0Only(false)` → `setPriorityFilter(new Set())`。

### 渲染：替换"💡 P0" chip 为多选 chip 行

原单 chip：
```tsx
{p0Count > 0 && <span ...💡 P0 (N)...>}
```

替换为多选行（priority asc）：

```tsx
{priorityCounts.length > 0 && priorityCounts.map(([p, count]) => {
  const active = priorityFilter.has(p);
  const togglePriority = (n: number) =>
    setPriorityFilter((prev) => {
      const next = new Set(prev);
      if (next.has(n)) next.delete(n);
      else next.add(n);
      return next;
    });
  return (
    <span
      key={p}
      role="button"
      tabIndex={0}
      onClick={() => togglePriority(p)}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          togglePriority(p);
        }
      }}
      title={
        active
          ? `再次点击移出 P${p} 过滤集合（多选）`
          : `加入到只看的 priority 集合（多选）：P${p}（${count} 条活动任务）`
      }
      style={{
        fontSize: 11,
        padding: "2px 8px",
        borderRadius: 10,
        background: active ? "#cbd5e1" : "#f1f5f9",
        color: "#475569",
        cursor: "pointer",
        whiteSpace: "nowrap",
        userSelect: "none",
        border: `1px solid ${active ? "#94a3b8" : "#e2e8f0"}`,
      }}
    >
      {active ? "✓ " : ""}{p === 0 ? "💡 P0" : `P${p}`}
      <span style={{ fontSize: 10, opacity: 0.7, marginLeft: 2 }}>
        ({count})
      </span>
    </span>
  );
})}
```

复用既有 P0 chip 的视觉规格（slate / gray 中性色，与决策日志 chip 的鲜
艳 accent 不同 —— priority 是结构化数字，不是 kind 类型 enum，弱色更
合适）。

### 测试

无单测；手测：
- 默认无选中：列表全显
- 点 P0 → 只显 P0 任务；同时 P0 chip 出 "✓ "
- 再点 P3 → 显 P0 + P3 OR 集合
- 点 ✕ 全部清掉过滤 → priorityFilter 重置
- 完成 P0 任务后切到 unfinished 视图：P0 chip 消失（counts 为 0）
- showFinished 切换：counts 派生自非完成集合，不被 finished 任务污染

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | state / memo / filter / filtersActive / clear-all 替换 |
| **M2** | render 替换 P0 chip 为多选行 |
| **M3** | tsc + build |

## 复用清单

- 既有决策日志 R83 多选 Set 模式
- 既有 dueFilter chip 视觉规格
- 既有 isFinished / 全 tasks state

## 进度日志

- 2026-05-09 09:00 — 创建本文档；准备 M1。
- 2026-05-09 09:08 — M1 完成。`p0Only` → `priorityFilter: Set<number>`；`p0Count` useMemo → `priorityCounts: [number, number][]` Map.entries asc 序；filteredTasks `.filter` 改 `priorityFilter.size === 0 || priorityFilter.has(...)`；`filtersActive` 改 `priorityFilter.size > 0`；clear-all-filters 改 `setPriorityFilter(new Set())`。
- 2026-05-09 09:11 — M2 完成。dueFilter chip 行内 `p0Count > 0 && <span ...💡 P0...>` 替换为 `priorityCounts.map([p, count])` 多选 chip；P0 保留 💡 glyph，其它走 `P{n}` 朴素文案；OR 命中 + (count) 显式 + active "✓ " 前缀；slate / gray 中性色与 dueFilter 一致。
- 2026-05-09 09:14 — M3 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (500 modules, 980ms)；grep 验证无遗留 p0Only / p0Count。归档至 done。
