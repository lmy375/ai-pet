# PanelTasks chip-bar「📋 audit · N」chip（iter #596）

## Background

iter #587 加 TG /audit_summary — 单命令聚合 5 大 audit 信号。本 iter
补桌面端对偶 — chip-bar 单 chip hover 看紧凑 summary + click 复制 md
文本。

## Change

`PanelTasks.tsx` chip-bar 起点（在 errorTaskCount 之前）加 📋 chip：

```tsx
const lines = [
  `📋 audit summary（${todayISO}）`,
  `· 📌 pinned: ${pinnedCount} 条 active`,
  `· 💤 idle 7d+: ${idleCount} 条 stale pending`,
  `· 🚀 今日 P7+: ${todayActiveP7Count} 条`,
  `· 🏷 近 7d rename: ${renameCount7d ?? 0} 次`,
  `· ✅ 今日完成: ${completionStats.today} 条`,
];
const total = pinnedCount + idleCount + todayActiveP7Count
  + (renameCount7d ?? 0) + completionStats.today;
// chip: 📋 audit · {total}
// hover: tooltip = lines.join("\n")
// click: copy to clipboard
```

## Key design decisions

- **5 信号与 TG /audit_summary 一致**：pinned / idle / today P7+ / 7d
  rename / today done — 跨 surface 同 audit 维度
- **复用既有 useMemo**：pinnedCount / idleCount / todayActiveP7Count
  已在 PanelTasks 内 compute；renameCount7d / completionStats 也 existing
  state — 不增 backend IO
- **tooltip 多行 summary + click 复制 md**：hover 看 + click 粘双 entry。
  与 TG /audit_summary 输出格式一致让跨 surface 切换无认知成本
- **总数 chip text `audit · {total}`**：让 chip 自身有数字信号（不全
  靠 tooltip）。total 是粗略 sum — 不精确语义但反映 audit 信号量
- **slate-tint 中性 fg-10%**：与 chip-bar 其它色族（amber / red /
  green / blue）错开 — 总览 chip 应中性不喧宾夺主
- **位置首位 chip-bar**：与 chip-bar 「navigation entry」语义一致 —
  owner 第一眼看到 audit 入口
- **chip-bar visibility gate 不变**：📋 chip 仅在 chip-bar 浮（即至
  少一信号 > 0）时显；不单独触发 chip-bar 显示（avoid 0 状态 chip
  孤独显示）

## Verification

- `npx tsc --noEmit` clean
- 视觉手测 deferred — chip 加在熟悉 chip-bar 位置，复用 existing
  state，无 layout race

## Future iters (out of scope)

- **chip click 弹 modal 展开**：当前 click 复制 md；future 可 click
  弹 modal 含 deep dive 入口按钮（与 TG /audit_summary 「→ /streak_pin
  / /idle_7d」一致）。但 modal 增交互成本
- **「audit · 高 N」red tint 告警**：total > 阈值（idle 多 / error 多）
  时切红 tint。需经验阈值；按需 propose
- **per-signal 子 chip 可 click 切对应 filter**：📌 → pinnedFilter；
  💤 → idleFilter。子 chip click 触发已有 filter — 一键直达。需将
  tooltip 行变 button
- **PanelMemory「📊 audit chip-bar」**：sibling — memory 维度 audit
  概览 chip。当前 TODO 已含此项
