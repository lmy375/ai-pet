# PanelTasks「☑️ 全选 P7+ 进 multi-select」chip（iter #359）

## Background

PanelTasks 已有两个高优 backlog 操作入口：
- 🎯 P7+ filter chip（line ~6878）—— 只改视图，缩窄到 P7+ pending
- ⌘A 全选 visible（handleSelectAllVisible @ ~4596）—— 选区跟随视图

owner 想"批量改 P7-P9 优先级 / 给所有高优加 #urgent tag"的常见
工作流目前需两步：先开 🎯 P7+ filter → 再 ⌘A 全选。本 iter 加
「☑️ 全选 P7+」chip 一键合成 — 跨当前视图直接把所有 P7+ pending
压进 selected Set 进入 multi-select 模式，省一步。

## Changes

### `src/components/panel/PanelTasks.tsx`（~line 6917）

🎯 P7+ filter chip 紧邻插入新 chip：

```tsx
{priorityBands[0].pending > 0 && (() => {
  const p7Titles = tasks
    .filter((t) => t.priority >= 7 && t.status === "pending")
    .map((t) => t.title);
  const matchesP7 =
    p7Titles.length > 0 &&
    selected.size === p7Titles.length &&
    p7Titles.every((tt) => selected.has(tt));
  const handle = () => {
    if (matchesP7) {
      setSelected(new Set());
      setBulkResultMsg("已清除 P7+ 选区");
    } else {
      setSelected(new Set(p7Titles));
      setBulkResultMsg(`已选中 ${p7Titles.length} 条 P7+ 进 multi-select`);
    }
    window.setTimeout(() => setBulkResultMsg(""), 2500);
  };
  return (
    <span role="button" onClick={handle} ...>☑️ 全选 P7+</span>
  );
})()}
```

设计要点：
- **源数据走完整 `tasks`，不走 `visibleTasks`** — 与 ⌘A 关键区别。
  owner 在任何视图（如 "本周 due" filter 下）也能一键 batch 全部
  P7+。这是本 chip 的核心价值 —— 否则就和 🎯+⌘A 两步等价了。
- **仅 pending status**：done/cancelled/error 不该被批量改 priority
  （cancelled 改 P9 没意义；error 该走 retry 路径不是 batch action）
- **toggle 行为**：selected 正好等于 P7+ 集合时再点 → 清空（与 ⌘A
  toggle 同心智）
- **`priorityBands[0].pending > 0` 渲染门槛**：与 🎯 P7+ filter 同
  策略 — 没有 P7+ 活动任务时不渲 dead chip
- **rose tint 同色族 + dashed border**：与 🎯 P7+ filter 视觉相近
  （表明同一语义簇"P7+ ops"），dashed vs solid 区分 select 动作 vs
  filter 状态。glyph `☑️` 直接传达 "checkbox tick" → "select" 语义
- **aria-pressed**：matchesP7 时为 true，让屏幕阅读器播 toggle 状态

## Key design decisions

- **位置紧邻 🎯 P7+ filter**：两个 chip 都是"P7+ ops"语义簇，并排
  让 owner 心智成组识别 — filter vs select。
- **不抽 helper `selectAllAtPriority(>=N)`**：诱惑加参数化"任意阈
  值 select all" — 但目前只有 P7+ 是 owner 高频用例，YAGNI。如未来
  要 P4+ mid-pri batch 再抽。
- **不在 batch action bar 内加入口而是顶部 chip 行**：batch action
  bar 仅在 selected.size > 0 时浮出 — 但本 chip 的核心场景是"从 0
  选区进 batch 模式"，需要在 selected.size === 0 时可见。
- **bulkResultMsg 反馈非 toast**：复用既有 bulkResultMsg state
  + 2.5s 自动清，与其它 batch 动作风格一致。
- **跨视图选区可能令 owner 困惑**："我开了 dueFilter='today'，怎么
  ☑️ P7+ 选了 17 条但视图只显 3 条？" → title attribute 已明示
  "（跨当前视图筛选）"。如未来反馈强烈再考虑"动 highPriorityOnly
  filter 同步缩窄视图"行为，但当前坚持"select 不动 view"职责分离。

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.22s)
