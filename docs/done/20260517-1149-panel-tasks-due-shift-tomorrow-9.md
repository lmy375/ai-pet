# PanelTasks 调期 popover 加「🌅 明早 09:00」preset（iter #288）

## Background

PanelTasks 行内 📅 调期 popover 已有 +1h / +1d / +3d / +1w / +2w 五个**相对
增量** preset。但 owner 最常见的 reschedule 模式是"推到次日早上开工时"
而非"现在 +1 天"（后者是现在 + 24h，假设现在 15:00 → 明天 15:00，不是
工作 morning 时刻）。

本迭代加「🌅 明早 09:00」**绝对锚点** preset：调既有 `dueTomorrow(now)`
helper 算出明早 9 点的 datetime-local 字符串。让"明天再说"一步搞定。

## Changes

仅 `src/components/panel/PanelTasks.tsx`：

- **`presets` 数据结构重构**：原 `{ key, label, deltaMs: number | null }`
  改为 `{ key, label, compute: (now: Date) => string | null }`：
  - 相对增量 5 个保留，compute 写成 `formatDueInput(new Date(now.getTime()
    + Xms))`
  - 新增 `{ key: "tomorrow9", label: "🌅 明早 09:00", compute: dueTomorrow }`
    放在 `+1h` 后、`+1d` 前（按"近 → 远"语义）

- **click handler 简化**：原 `p.deltaMs === null ? null :
  formatDueInput(...)` 改为 `p.compute(new Date())` — compute 内部自己决
  定返 string | null。

- **chip tooltip 更新**：提示新增「🌅 明早 09:00」锚点 preset。

## Key design decisions

- **改成 compute 函数式而非新增字段**：允许 preset 表达两类语义（相对 +
  绝对）；未来加"今晚 18:00" / "周一 09:00" 等更复杂锚点也能复用同 shape，
  不必让 callsite 写多个 fallback 分支。
- **🌅 emoji + "明早"**：与 snooze popover 的「💤 至明早 09:00」对偶 ——
  那是"暂存到明早"，这是"due 调到明早"，二者语义不同但都用 9:00 morning
  锚点；emoji 也用 🌅（snooze 是 💤）让两条 chip 视觉区分。
- **复用 dueTomorrow**：既有 helper 已实现"明早 9:00（按工作日开工时刻
  / 让 owner 不用打 datetime-local picker）"。Snooze popover 也用此 helper
  保持一致。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.23s)
