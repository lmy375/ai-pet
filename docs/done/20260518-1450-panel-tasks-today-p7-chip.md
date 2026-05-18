# PanelTasks chip-bar「🚀 今日 P7+」chip（iter #571）

## Background

PanelTasks 已有 🎯 紧迫 chip 显「全谱 P0-P2 未完成」— 老积压的紧迫
视角。owner 缺「今日新增高优」信号 — 早会前 / sprint 起步前一眼看
「今天我新加了几条紧急活」。两者正交：
- 🎯 紧迫：日期不限 + priority 0-2 + 未完成 = **积压视角**
- 🚀 今日 P7+：created_at 今日 + priority ≥ 7 + 活动态 = **新增视角**

## Change

`PanelTasks.tsx` 加 `todayActiveP7Count` useMemo（紧贴
`pinnedCount` / `idleCount` 同族）+ 在 chip-bar 紧贴 🎯 紧迫 chip 后
插 🚀 chip：

```tsx
const todayActiveP7Count = useMemo(() => {
  const now = new Date(nowMs);
  const todayPrefix = `${y}-${m}-${d}`;
  let n = 0;
  for (const t of tasks) {
    if (isFinished(t.status)) continue;
    if (t.priority < 7) continue;
    if (t.created_at.length < 10) continue;
    if (t.created_at.slice(0, 10) !== todayPrefix) continue;
    n += 1;
  }
  return n;
}, [tasks, nowMs]);
```

chip-bar 可见 gate `|| todayActiveP7Count > 0`。

## Key design decisions

- **P7+ 而非 P0-P2**：与既有「高优 priority chip」family 一致 — P7-P9
  是高优 in this codebase（P0 最低 / P9 最高 in some conventions but
  here P7+ 是「紧急 sprint」级，从 task_set_pinned / promote_all_p7
  /pin_all_p7 等命令名可见）
- **活动态 only**：done / cancelled 不算「今日 active」；error 仍算
  （需注意但未完成 — 与 sprint audit 语义一致）
- **purple tint**：与既有 🎯 amber / 📌 amber / 💤 red / 🔥 green
  色族错开 — 让 owner 扫 chip-bar 时不混淆 P7+ 与紧迫 / pinned 等
- **informational 不接 filter**：与 🎯 同 model — chip 是信号显示，
  不切 view。owner 想 filter 走既有 P{n} chips 多选 + highPriorityOnly
  toggle
- **0 时不渲**：与所有 count chip 同稀疏模板
- **`nowMs` 已 1s tick**：跨午夜自动归零（next day 0:00 后今日 prefix
  变，全部 created_at 今日的 task 失配）

## Verification

- `npx tsc --noEmit` clean
- 视觉手测 deferred — 单 chip 加在熟悉 chip-bar 位置（紧贴 🎯
  紧迫），同 model 不引入新交互

## Future iters (out of scope)

- **🚀 chip click 切到「今日 + P7+」filter**：从信息性 chip 升级为
  filter toggle — 让 owner audit「具体哪几条」一键聚焦。但 informational
  vs filter 在 chip-bar 已有 model 混用（DueChip 是 filter，🎯 是 info）
  — 需先理清统一约定再做
- **TG `/today_p7`**：远程同 audit；与 /idle_7d / /pinned_drop_7d 等
  7d audit 族不同尺度（today vs 7d）。按需 propose
- **「P7+ 数 / P0-P2 总数」比率 chip**：sprint 节奏指标 — 但比率信号
  含义偏抽象，owner 可能更需具体计数
