# PanelMemory item action row「📅 created N前」hover chip（iter #358）

## Background

PanelTasks 任务行已有「📅 N前 / 🕰 N前」inline chip（PanelTasks.tsx
~8504-8523）— owner glance 知"这条任务何时 enqueue"。PanelMemory 此前
只在"展开详情后"显 created / updated 时间（line 4766+），收起态没
有任何创建时间感知。本 iter 加 collapsed-state hover chip，对偶
PanelTasks 行内 N前 — owner 不必逐条展开就能 glance "这条 user_profile
是 3 个月前的老条目还是上周新建的"。

## Changes

### `src/components/panel/PanelMemory.tsx`（action row, ~5675）

action row 容器 `<div style={{ display: "flex", gap: 4 }}>` →
追加 `alignItems: "center"`（防 inline-flex 子元素与 button 顶对齐
时基线漂移）。

在 pin / ▶️ / ⏭ 等 action buttons 前插一段 IIFE：

```tsx
{(() => {
  const ts = Date.parse(item.created_at);
  if (Number.isNaN(ts)) return null;
  const nowMs = Date.now();
  const ageMs = Math.max(0, nowMs - ts);
  const rel =
    ageMs < 60_000
      ? "刚刚"
      : formatRelativeAgeBuckets(ageMs);
  return (
    <span
      title={`创建于 ${item.created_at.slice(0, 16).replace("T", " ")}（${rel}）— hover info...`}
      style={{
        display: "inline-flex",
        alignItems: "center",
        fontSize: 10,
        lineHeight: 1.4,
        color: "var(--pet-color-muted)",
        opacity: 0.7,
        whiteSpace: "nowrap",
        fontFamily: "'SF Mono', monospace",
        marginRight: 2,
      }}
      aria-label={`已创建 ${rel}`}
    >
      📅 {rel}
    </span>
  );
})()}
```

设计要点：
- `position` 在 action buttons 之前 → 视觉"info first，actions after"
- `opacity: 0.7` + muted color + no bg / no border → 视觉降权，不抢
  action button 注意力（与 PanelTasks line 8504 "新进 hint" 同款）
- `marginRight: 2` → 与第一个 action button 之间额外 2px → 视觉分组
- `title` attribute 给完整 created_at 前 16 字符（YYYY-MM-DD HH:MM）+
  相对值 + "hover info 不可交互" 提示 — 与 expanded 详情元数据互补
- `Date.parse` 容错：非标准 ISO 时 silent skip（item.created_at
  在 v3 之前的旧 yaml 可能格式不一致，不该阻塞 chip 渲染）
- 复用既有 `formatRelativeAgeBuckets` util（与 PanelTasks 同 import）
- `ageMs < 60_000 → "刚刚"`：与 PanelMemory ~4779 fmt 函数同心智 —
  避免新建条目刚出生就显"0 分钟前"的尴尬

## Key design decisions

- **位置：action row 起始 vs 末尾 vs title row 尾**：
  - title row 尾（tag chips 之后）— 与 #tag chip 视觉混淆（都 muted
    color），且 title row 当 description 长时本身已挤
  - action row 末尾（删除按钮后）— 信息性 chip 紧贴危险按钮，视觉
    紧张感不必要
  - **action row 起始**（采用）— "info → actions" 自然阅读顺序，
    且与 PanelTasks 行内 N前 chip 同位置心智（PanelTasks 的 N前 也
    在 priority button / 删除等 actions 之前）

- **不复用 PanelTasks 的 3 天阈值切换 "🕰" actionable 风格**：
  PanelTasks 把 ≥3 天的任务标记为 actionable "积压"信号，是因为
  pending task 有"该拆 / 该取消 / 该升级"诉求。memory item 没有
  类似"积压"语义 — 一条 3 个月前的 user_profile 完全正常，不该
  被标"⚠️ 积压"。所以只保留 info chip 单态。

- **不显 updated_at 的 N前**：updated_at chip 是 sortByRecent
  toggle 的隐含信号，且 description 编辑频繁会让 updated_at 飘移
  → 不稳定，对 owner 决策价值低。created_at 是"出生时刻"，稳定，
  glance 价值高。

- **不抽 helper 到 utils**：JSX 一段 35 行的 IIFE 内联够清晰，抽
  helper 要传 4-5 个 style 字段，反而增加 surface area。如未来
  PanelChat / PanelInsights 也要类似 chip 再抽。

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.24s)
