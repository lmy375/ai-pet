# PanelMemory section header 🔇 silent chip 改可点 toggle 仅显 silent + 上轮 TODO 漏检替换

## 背景

### 上轮 TODO #4 stale 移除

上轮 auto-propose 的 "PanelTasks 任务行右键菜单加「📋 复制 title 作 ref token」简短按钮" 是 stale —— grep 显示既有 "🔗 复制为 ref（「title」）" 按钮（PanelTasks.tsx:10116）已实现同语义。移除该项。

教训：auto-propose 阶段必须 grep 验证，与既往多次 stale TODO 发现一致。

### 本 iter 实现：silent filter toggle

iter #197 加了 PanelMemory butler_tasks section header 上的 🔇 N silent 计数 chip（纯展示）。owner 看到"5 条 silent"想"具体是哪 5 条" —— 必须滚整段筛选。

把该 chip 改为可点 toggle，激活时仅显该 cat 的 [silent] 任务，让 owner 一键 inspect + 决定是否调整 / 解除。

## 改动

### `src/components/panel/PanelMemory.tsx`

#### 1. 新 `silentOnlyCats: Set<string>` state

```ts
const [silentOnlyCats, setSilentOnlyCats] = useState<Set<string>>(new Set());
```

- 多 cat 独立 set（虽现仅 butler_tasks 有 silent 语义，泛化为 cat-level 不增本 iter scope）
- 不持久化：filter 是临时 inspect 视图，跨 session 默认全显更符合直觉

#### 2. 把 silent 计数 chip 从 `<span>` 改为 `<button>` + toggle handler + active 视觉态

```tsx
<button
  onClick={() => setSilentOnlyCats(prev => toggle prev catKey)}
  style={{
    ...chipBase,
    border: active ? "1px solid accent" : "1px solid transparent",
    background: active ? "blue tint" : "gray border",
    color: active ? "blue fg" : "muted",
    fontWeight: active ? 600 : 400,
    cursor: "pointer",
  }}
  title={active ? "...点击恢复显全部" : "...点击仅看 N 条 silent 任务"}
>
  {active ? "✓ " : ""}🔇 {silentN}
</button>
```

- active 态：accent 边框 + 蓝 tint bg + accent fg + 粗体 + ✓ 前缀 —— 与其它 active filter chip 同视觉语言
- 非 active：原灰 muted bg（保留 iter #197 既有视觉）

#### 3. `scheduleFilteredItems` 接入新 filter

```ts
const scheduleFilteredItems = (() => {
  let pool = cat.items;
  // 🔇 silent filter：与 schedule kind filter AND 关系叠加
  if (silentOnlyCats.has(catKey)) {
    pool = pool.filter(it => /\[silent\]/.test(it.description));
  }
  if (catKey === "butler_tasks" && butlerScheduleFilter.size > 0) {
    pool = pool.filter(...既有 schedule kind 过滤...);
  }
  return pool;
})();
```

silent filter 走 pool 第一层 → 与 schedule kind filter AND（owner 选 "silent + every" 时仅看周期性静默任务）。

## 关键设计

- **复用 silent 计数 chip 作 toggle 入口**：既已显数字，再 click → 仅显示这 N 条，最少 UI 元素 + 最直接交互。
- **AND-stack 与 schedule filter**：silent + every 同选时叠加 —— filter 语义可组合，owner 玩自由筛选。
- **不持久化 Set**：filter 是 inspect 视图，跨 session 应 reset 回"全显"默认。与 `expandedCategories` 同 ephemeral state 模式。
- **cat-level Set**：silentOnlyCats 用 `Set<string>` 而非 `boolean`，让"多 cat 同时 silent-only"成为可能 —— 虽当前 butler_tasks 是唯一有 silent 语义 cat，但泛化设计不增成本。
- **active 视觉态参考其它 active filter**：accent border + 蓝 tint bg + ✓ 前缀 —— 与 PanelTasks 顶 priority / tag chip 等 active filter chip 视觉语言一致。
- **不引入 OR semantics with other filters**：当前 schedule + silent 都 AND，符合"filter 越选越窄"的直觉。OR 留给后续 chip group 设计。
- **不写测试**：纯 frontend filter pipeline；既有 schedule filter 路径无 frontend 单测（项目无 frontend test runner）；视觉验证（创建 [silent] task → click chip → 仅显该 task）足够。

## 不做

- **不让"非 butler_tasks" 也支持 silent filter**：silent marker 在其它 cat 无 backend 语义；显 filter 仅徒增 UI 噪音。
- **不把 silent filter 独立到 panel 顶级 chip row**：butler section 内 silent 数据 + 操作集中，跳出本段反而割裂。
- **不绑键盘快捷**：silent 是相对罕用 filter（owner 不每 session 都 inspect）；右键菜单 / 节内 chip 足够。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.16s
- 改动 ~70 行（silentOnlyCats state 7 + chip → button + 视觉态 35 + scheduleFilteredItems IIFE 改写 22 + 注释）。既有 butlerScheduleFilter / pinnedKeys / 排序逻辑 / shownItems 折叠 / sortByRecent 完全不动；silent 计数显示在 silent_count > 0 时仍同样。

## TODO 状态

剩 3 条留池：
- detail.md 编辑器 toolbar 加「📤 复制 LLM consume 段」按钮
- butler_task `[every:]` 解析 "工作日 09:00" / "周末 10:00" 周内限定
- PanelMemory item hover preview tooltip 加双击编辑 onboarding hint（实际上轮 #201 已实现）

待会清理重复条目。

## 后续

- 同款 active-toggle 给 💤 snooze chip：click → 仅显 active snooze 任务。
- 加 "🔇 + 🔍 mini popup" 显具体 silent items 标题列表 + 一键解除 silent 按钮（与 pinned 段下拉同模板）。
- silent / snooze filter 联动 telegram bot `/silent` `/snooze` 命令 —— 让 TG 也能 list / unset。
- 类似的 filter 模式拓展到其它 cat 的潜在 marker（如 ai_insights 的 [draft] hint marker 等）。
