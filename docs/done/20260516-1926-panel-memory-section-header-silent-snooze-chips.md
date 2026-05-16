# PanelMemory butler_tasks section header 加 🔇 silent / 💤 snooze 计数 chip

## 背景

iter #193 引入 `[silent]` marker，iter 早期已有 `[snooze: YYYY-MM-DD HH:MM]`。PanelMemory 行级别有 🔇 silent chip + 💤 snooze 已显示。但 section header 只有 cat.items 总数 + 最近时间 + 7d sparkline —— owner 想知道"这个 cat 里有多少条被静默 / 暂停"必须滚下来看每行。

加 header 级别 silent / snooze 计数 chip，与已有 pinned section pinned chip 同模板，让 owner 一眼看到管家队列里有多少 actionable vs sleeping。

## 改动

### `src/components/panel/PanelMemory.tsx`

butler_tasks section header 在既有 items 数 badge 后插入：

```tsx
{catKey === "butler_tasks" && (() => {
  let silentN = 0;
  let snoozeN = 0;
  const nowMs = now.getTime();
  const snoozeRe = /\[snooze:\s*(\d{4})-(\d{2})-(\d{2})\s+(\d{1,2}):(\d{1,2})\]/g;
  for (const it of cat.items) {
    if (/\[silent\]/.test(it.description)) silentN += 1;
    // last-wins: 多个 snooze marker 取最后一个 valid
    let lastUntilMs: number | null = null;
    snoozeRe.lastIndex = 0;
    let m: RegExpExecArray | null;
    while ((m = snoozeRe.exec(it.description)) !== null) {
      const d = new Date(+m[1], +m[2]-1, +m[3], +m[4], +m[5]);
      if (!Number.isNaN(d.getTime())) lastUntilMs = d.getTime();
    }
    if (lastUntilMs !== null && lastUntilMs > nowMs) snoozeN += 1;
  }
  return (
    <>
      {silentN > 0 && <span style={muted gray chip} title="...">🔇 {silentN}</span>}
      {snoozeN > 0 && <span style={blue tint chip} title="...">💤 {snoozeN}</span>}
    </>
  );
})()}
```

显示规则：
- silent: 严格字面 `[silent]` 计数
- snooze: 多 marker 取最后一个 valid + 仅算未过点（active）—— 与 backend `snoozed_until_map` 同语义
- 0 计数不渲染（与 既有 pinned chip 模板一致）
- silent chip muted gray bg / 💤 snooze chip blue tint bg —— 与行级别 chip 风格各自对偶

## 关键设计

- **仅 butler_tasks cat 渲染**：其它 cat（user_profile / ai_insights / general / todo）有 silent/snooze marker 无语义意义（不进 proactive cycle）。
- **snooze active-only 语义**：与 backend `task_queue::snoozed_until_map` 一致 —— 过点 marker 自然失效不计入，避免"3 个月前的过期 snooze 仍计"噪音。
- **last-wins parse**：多个 snooze marker 取最后一个有效值（LLM append 新 marker 不需先剥旧，本统计跟上）。
- **正则 nowMs 由外层 `now: Date` 注入**：与 header 既有 latestTs / formatLastUpdated 共享 now 一致 anchor。
- **0 不渲染**：每 iter 新增 1-2 chip 时 header 容易"虚胖"，0 即 hidden 让 header 仅显有信号的 chip，与既有 `latestTs !== null` / `cat === butler_tasks && overdueCount > 0` 同 sparse-chip 模板。
- **位置紧贴 items 数 badge**：silent / snooze / pinned 都是"队列里的子状态"，与基础 items 数同 visual cluster；与 7d sparkline / 最近时间（updated_at 信号）拉开距离。

## 不做

- **不在 todo / 其它 cat 渲**：见上。
- **不显 silent + snooze 同时合并为一段**：两个 marker 独立维度（silent = owner 意图；snooze = 时刻），分两个 chip 让 owner 一眼区分。
- **不显已过点 snooze**：过点 = 失效 = 该 task 重新出现在 proactive 选单。统计 active = "现在仍 sleeping" 才有意义。
- **不写测试**：纯 regex + Date 算术；视觉验证（butler_tasks cat 里加 [silent] + [snooze: 未来时间] 各一条 → section header 应显 🔇 1 + 💤 1）足够。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.16s
- 改动 ~80 行（IIFE + 两 chip 渲染 + 注释）。既有 items 数 / latestTs / 最近时间 / 7d sparkline / 闲置 hint / overdue 计数按钮 / 立即处理 按钮 路径完全不动。

## TODO 状态

剩 4 条留池：
- PanelTasks 行右键加「🔇 Toggle silent」一键 toggle
- 桌面 pet collapse tab hover 1s 浮 ambient mini card
- detail.md 编辑器底 status bar 加 字数统计 chip
- butler_task `[snooze: ...]` 支持自然短串预设

## 后续

- silent / snooze chip click → 弹一个 mini list 列具体哪几条任务被静默 / 暂停（与既有 pinned filter chip 同一交互模式）。
- 在 PanelTasks 顶部 tone strip 也加 silent / snooze 全局计数 chip，让面板切换 panel 也保留这个信号。
- 加 silent / snooze 数变化的 sparkline 趋势（与 7d churn sparkline 互补）—— 让 owner 看到"上周比这周多/少多少静默任务"。
