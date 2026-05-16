# PanelTasks 顶 chip 行加 "🎯 紧迫 N" P0-P2 计数 chip

## 背景

PanelTasks 顶 chip 行已显 🔴 逾期 / 📅 今日到期 / 📌 钉 / ✓ 完成 等计数信号，但缺"高优先级 backlog 总览"。owner 看 list 时只能逐档 P-pill 计数估总量，决策"今天该优先做哪个"成本高。

加 🎯 紧迫 N 综合显示 P0-P2 未完成任务数 —— 让 owner 一眼知道"队列顶端有几条要重做"。

## 改动

### `src/components/panel/PanelTasks.tsx`

#### 1. 新 `urgentTopPriorityCount` useMemo

```ts
const urgentTopPriorityCount = useMemo(() => {
  let n = 0;
  for (const t of tasks) {
    if (isFinished(t.status)) continue;
    if (t.priority <= 2) n += 1;
  }
  return n;
}, [tasks]);
```

#### 2. amber chip 渲染（紧贴 ✓ 今日完成 chip 前）

```tsx
{urgentTopPriorityCount > 0 && (
  <span
    style={{
      fontSize: 11, padding: "2px 8px", borderRadius: 8,
      background: "var(--pet-tint-amber-bg, var(--pet-tint-yellow-bg))",
      color: "var(--pet-tint-amber-fg, var(--pet-tint-yellow-fg))",
      fontWeight: 600, whiteSpace: "nowrap",
    }}
    title={`高优先级 (P0-P2) 未完成任务 N 条。owner 应优先处理这些；queue 顶有积压时考虑暂缓低优先级。`}
  >
    🎯 紧迫 {urgentTopPriorityCount}
  </span>
)}
```

#### 3. 外层 chip 行 gate 加 urgentTopPriorityCount > 0

```diff
- (dueTodayCount > 0 || overdueCount > 0 || ... || completionStats.today > 0)
+ (dueTodayCount > 0 || overdueCount > 0 || ... || completionStats.today > 0 || urgentTopPriorityCount > 0)
```

## 关键设计

- **P0-P2 阈值**：P0 (最紧急) / P1 / P2 三档定为"紧迫"。owner 心智 = "顶尖 30% 优先级"。
- **amber tint**：介于 red overdue / green done / blue stats 之间 —— "需要注意但未必燃眉"信号。amber 变量带 yellow fallback 防主题缺失。
- **0 不显**：与其它计数 chip 同稀疏模板。
- **informational 不绑 filter**：既有 P-pill chip 已支持单档 click 筛；本 chip 是"总览"信号无需重复 filter UI。
- **isFinished 过滤**：与 priorityCounts memo 同活动态语义 —— done / cancelled 不计入。

## 不做

- **不绑 click → 筛 P0-P2 视图**：scope creep；既有 P-pill 已能筛单档，"P0-P2 多选"走 selectedPriorities 多选模式。
- **不写测试**：纯 useMemo + conditional render；视觉验证（造几条 P0/P1/P2 pending task → chip 应显总数）足够。
- **不显具体每档分布（P0:1 / P1:2 / P2:0）**：既有 priorityCounts.map 已逐档显示；本 chip 取总和减少视觉密度。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.21s
- 改动 ~25 行（useMemo 12 + chip render 18 + gate +1 条件）。既有 priorityCounts / dueChips / completionStats chip / pinned chip 路径完全不动。

## TODO 状态

剩 4 条留池：
- PanelMemory ⌘K 唤起跨 cat memory quick-find palette
- ChatMini bubble hover 浮 "💾 转 task" 按钮
- detail.md 编辑器 toolbar 加 "🔍 detail 全文搜" 浮 search bar
- PanelTasks 列表行底加 "⏰ 还 N 分钟" 倒计时（due ≤ 60 分钟）

## 后续

- 阈值可配置：让 owner 在 Settings 改 "紧迫定义 = P ≤ ?" 让"我把 P3 也当紧迫"风格 owner 自定义。
- chip click → temporarily 切 priorityFilter 显 P0-P2 视图 + 再 click 还原。
