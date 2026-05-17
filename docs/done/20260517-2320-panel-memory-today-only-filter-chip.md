# PanelMemory「🆕 仅今日」filter chip toggle（iter #338）

## Background

PanelMemory 已有「🌱 今日新增 N」按钮 click 弹 drill-down modal 列今日
按 cat 分段清单。但 owner 想在 panel 视图内"过滤"看今日 items（保留
cat 树结构 / 既有 hover preview / 编辑入口）时需要先关 modal → 走完整
搜索 → ...，工作流不顺。

本迭代加「🆕 仅今日」toggle chip — 与既有 🌱 chip 互补：
- 🌱 = drill-down modal（清单视图）
- 🆕 = filter（panel 树内只显今日 items）

## Changes

仅 `src/components/panel/PanelMemory.tsx`：

- 新 state `todayOnlyFilter: boolean` + localStorage 持久（key
  `pet-memory-today-only`；与 sortByRecent / sortBulterByNextFire 同
  pattern）+ `toggleTodayOnlyFilter` 函数
- `scheduleFilteredItems` IIFE 加新过滤段（在 inplaceFilter 之后 / silent
  filter 之前）：
  - `todayOnlyFilter` 真时按 `created_at.startsWith(today)` 过滤
  - `today` 用 `toLocaleDateString("sv-SE")` 拿本地 YYYY-MM-DD（与
    todayNewCount 同算法 — UTC vs 本地午夜不漂移）
- 在 🌱 chip 之后插「🆕 仅今日 N」toggle 按钮：
  - 仅 `todayNewCount > 0` 时浮（无今日新增 toggle 无意义）
  - 激活态走 accent border + blue tint；未激活 muted gray
  - tooltip 区分态："已仅显..." vs "仅显..."
  - aria-pressed 让 a11y 透明

## Key design decisions

- **保留既有 🌱 drill-down chip**：drill-down modal 与 in-panel filter
  覆盖不同 use case — owner 想"快速一眼看清单"走 modal，想"在视图内
  按今日逐条 review / 编辑"走 filter chip。两者按 audit 风格分流。
- **gate `todayNewCount > 0`**：避免显"🆕 仅今日 0" 死按钮 — 与既有
  PanelTasks 🎯 P7+ chip gate on count > 0 同 pattern。
- **过滤段在 inplaceFilter 之后 silent 之前**：与既有过滤维度 AND 叠
  加 — owner 选 "仅今日" + 搜 "周报" → 命中今日新增中含 "周报" 的 items
  （符合直觉）；与 silent / schedule kind / sortByRecent 等也 AND。
- **localStorage 持久而非 session**：owner 切走再回到 panel 偏好保留 —
  与 sortByRecent / pinnedFilter 等 panel filter 习惯一致。
- **toLocaleDateString("sv-SE") 而非 toISOString().slice(0,10)**：sv-SE
  locale 输出 ISO YYYY-MM-DD 但走本地时区；后者用 UTC 会让"今天凌晨
  写的"被识别为"昨天"。与 todayNewCount 同算法保一致。
- **不引入 unit test**：scheduleFilteredItems IIFE 内嵌算法 + 既有
  todayNewCount 计算同 helper；通过 vite build + 真实交互验证。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.22s)
