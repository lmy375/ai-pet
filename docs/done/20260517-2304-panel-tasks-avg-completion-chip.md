# PanelTasks 顶部「⏱ 平均完成耗时」chip（iter #415）

## Background

PanelTasks 顶部既有量化信号 chip：`📌 N`（pinned 计数）、`🎯 P7+ N`
（高优活动计数）、`📅 N · M`（今日活跃 chat × items）等 — 都是
**次数 / 计数维度**。缺一个**耗时维度**信号：「我最近做完一条 task
平均花多久」— 衡量通量 / 流速。

owner 用既有 `/streak`（连续天数 + 完成次数）也只是次数信号；
"做完一条平均 4h vs 24h" 这种差异完全看不出。本 iter 加 ⏱ 平均
完成耗时 chip — 扫 done 且 updated_at 在近 30 天的 task，算
`(updated - created)` 平均小时。

## Changes

### `src/components/panel/PanelTasks.tsx`

#### 1. `avgCompletionHours` useMemo

```ts
const avgCompletionHours = useMemo<{
  avgHours: number;
  sampleCount: number;
} | null>(() => {
  const cutoff = nowMs - 30 * 24 * 60 * 60 * 1000;
  let total = 0;
  let n = 0;
  for (const t of tasks) {
    if (t.status !== "done") continue;
    const updated = Date.parse(t.updated_at);
    const created = Date.parse(t.created_at);
    if (Number.isNaN(updated) || Number.isNaN(created)) continue;
    if (updated < cutoff) continue;
    const hours = (updated - created) / 3_600_000;
    if (hours < 0) continue;       // 防数据脏：updated < created
    total += hours;
    n += 1;
  }
  if (n === 0) return null;
  return { avgHours: total / n, sampleCount: n };
}, [tasks, nowMs]);
```

设计要点：
- **窗口 = 近 30 天 updated_at**：与既有 30 天活动窗口（PanelTasks
  sparkline / butler_history）同节奏。done 时刻在窗口内即算 — 即
  使 task created 在 30 天前
- **过滤 NaN ts**：防异常时间戳让平均值 NaN
- **过滤 updated < created**：防数据脏 — 理论上不该发生但防御性
  skip 比让平均值偏移更稳妥
- **null 表示 "no samples"**：与既有 chip 模式同（0 样本 → 不渲），
  避免显「0h ⏱」误导
- **依赖 nowMs**：与 dueTodayCount / overdueCount 同 invalidation
  trigger — 30s 内重算（覆盖跨日 boundary）

#### 2. chip UI（紧贴 📌 pinnedCount 之后）

```tsx
{avgCompletionHours && (() => {
  const { avgHours, sampleCount } = avgCompletionHours;
  const label =
    avgHours < 1
      ? "<1h"
      : avgHours >= 48
        ? `${(avgHours / 24).toFixed(1)}d`
        : `${Math.round(avgHours)}h`;
  return (
    <span
      title={`近 30 天 ${sampleCount} 条 done task 的平均完成耗时...`}
      style={blue-tint-chip}
    >
      ⏱ 均 {label}
    </span>
  );
})()}
```

显示规则：
- `< 1h` → `<1h`（短时通量也 audit）
- `1h - 48h` → `Nh`（Math.round；颗粒到整 hour，紧凑）
- `≥ 48h` → `Nd`（1 位小数；天粒度更直觉）
- title attr 显完整 `${avgHours.toFixed(1)} 小时 over N 条` 详情

视觉：blue tint chip（与既有 P7+ rose / pinned amber / today blue
区分），`tabular-nums` 让数字对齐宽度稳定 — chip 不抖。

## Key design decisions

- **status == "done" only**：cancelled 不算（owner 不关心"我取消
  task 平均花几小时"）；error retry 后变 done 才算（最终通量信号）
- **30 天窗口固定不可调**：与既有 sparkline / butler_history 一致
  时间尺度；avg N 天可配置是过度设计 — owner 真要看不同窗口可走
  /streak（7d / 30d）或 future iter 加 panel 切换
- **不显「中位数」/「N50」**：avg 已被业界默认接受为通量指标；中
  位数对极端样本鲁棒但 chip 单值只能选一个，avg 更直觉（且大样本
  下两者接近）
- **不为单 chip 引 unit test**：纯派生计算 + 边界条件已显（NaN
  skip / < cutoff skip / hours < 0 skip / null 兜底）；build pass
  + 手测足够（确保 1 条 done task 在 30 天内 → chip 出 + label
  正确 ; 加多条耗时不同的 → 验平均；清空 done → chip 消失）
- **不写到 README**：内部 audit chip，与既有计数 chip 同 visibility
  级别 — 不算"product highlight"

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.24s)
- 后端无改动 — 纯前端 useMemo 派生
