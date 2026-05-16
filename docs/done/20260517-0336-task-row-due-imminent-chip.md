# PanelTasks 行内加 "⏰ 还 N 分" 即将到期倒计时 chip

## 背景

任务行 meta 区已显 "截止 YYYY-MM-DD HH:MM" + urgency 配色，但 owner 想知道"具体还剩几分钟" 时只能心算 (now → due)。

加 conditional chip "⏰ 还 N 分" 仅 due 在未来 ≤ 60 分钟时浮红 tint —— 让 owner 长列表时一眼看到"立即到期"急迫信号。

## 改动

### `src/components/panel/PanelTasks.tsx`

任务行 meta 区，截止 chip 之后插：

```tsx
{t.due && !isFinished(t.status) && (() => {
  const ts = Date.parse(t.due);
  if (Number.isNaN(ts)) return null;
  const diffMs = ts - nowMs;
  if (diffMs <= 0 || diffMs > 3_600_000) return null;  // 仅未来 ≤ 60 分钟
  const mins = Math.ceil(diffMs / 60_000);
  return (
    <span
      style={{
        background: "var(--pet-tint-red-bg)",
        color: "var(--pet-tint-red-fg)",
        padding: "1px 6px",
        borderRadius: 999,
        fontWeight: 600,
        fontFamily: "'SF Mono', monospace",
      }}
      title={`due 在 ${mins} 分钟后到期 — 立即处理。`}
    >
      ⏰ 还 {mins} 分
    </span>
  );
})()}
```

## 关键设计

- **gate 在 60 分钟内 + 未来**：> 1 小时走既有"截止 X 小时后" tooltip 已足；过期走 overdue red urgency。本 chip 仅覆盖"剩时间不到 1 小时"间隙 —— 是 acute urgency 信号。
- **isFinished gate**：done / cancelled 行 "还 N 分" 无意义。
- **`Math.ceil` 向上取整**：let "29 分 30 秒 → 还 30 分"对 owner 直觉 = "至少还有这么多"。
- **红 tint pill style**：与 overdue urgency 同 visual 警示等级 —— 因"快到期"和"已过期"都属"立即处理"信号。
- **`!Number.isNaN(ts)` 防御性**：t.due 形态 `YYYY-MM-DDThh:mm`，Date.parse 通常通；malformed 时静默不显。
- **依赖 nowMs**：现有面板 1 分钟级 refresh nowMs，chip 自然每分钟 -1 倒数。

## 不做

- **不实时秒级倒数**：每秒重渲噪音；分钟级足够紧迫感。
- **不写 toast 当 chip 跨阈值（60 分 / 0 分）**：scope creep；已有 [reminderMin] marker 软提醒覆盖"到点前 N 分钟"独立路径。
- **不绑 click → 跳 task focus**：行 click 已展开任务详情，chip 子 click 不该重复 path。
- **不写测试**：纯 conditional render；视觉验证（造一条 due 在 30 分后的 task → chip 应显 "⏰ 还 30 分"）足够。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.18s
- 改动 ~30 行（IIFE + chip + 注释）。既有 due chip / urgency 配色 / 创建于 chip / origin chip 路径完全不动。

## TODO 状态

剩 3 条留池：
- PanelMemory ⌘K 唤起跨 cat memory quick-find palette
- ChatMini bubble hover 浮 "💾 转 task" 按钮
- detail.md 编辑器 toolbar 加 "🔍 detail 全文搜" 浮 search bar

## 后续

- chip 加 click → 弹小 popup 让 owner 一键 "snooze 30m / mark done / open detail" 直接处理。
- ≤ 5 分钟时 chip 加微 pulse 动画提醒"真的就快到了"。
- 邻近多条任务 due 都在 60 分内时，顶 toneStrip 加 "🚨 N 条 1 小时内到期" 汇总 chip。
