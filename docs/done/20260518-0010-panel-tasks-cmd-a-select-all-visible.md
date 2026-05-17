# PanelTasks ⌘A 全选 visible 进 multi-select 模式（iter #341）

## Background

PanelTasks 已有 multi-select 机制（Space toggle 焦点行 / 鼠标 click 行
multi-select checkbox），但 owner 想"批量 cancel / done / pin 所有当前
visible 任务" 时只能逐条 click — 当 visibleTasks = 30 条时 friction
高。

本迭代加 ⌘A 一键全选当前 visibleTasks，再按 ⌘A toggle 清空。与既有
ctx menu / bulk action toolbar 工作流相辅相成。

## Changes

### `src/components/panel/PanelTasks.tsx`

- 新 callback `handleSelectAllVisible`：
  - `visibleTasks.length === 0` 短路（防 no-op 触发 toast）
  - 算 allTitles；setSelected 更新器内判断 `prev` 是否已是同集合
  - 已全选 → `new Set()` + toast "⌘A 清除全部选中"
  - 未全选 → `new Set(allTitles)` + toast "⌘A 全选 N 条"
- 位置在 `handleCopyVisibleTitles` 之后避开 `visibleTasks` TDZ 顺序
  问题（与 iter #307 同 lesson）

### `src/components/panel/useTaskKeyboardNav.ts`

- args 加 `handleSelectAllVisible: () => void` + ref + sync effect
- keydown 末段（在 ⌘E 之后）加 ⌘A 分支：
  - `(e.metaKey || e.ctrlKey)` + key=='a' + 无 shift / alt
  - `list.length === 0` 短路
  - preventDefault + 调 handleSelectAllVisibleRef.current()
- 位置在 tagName 守卫**之后** — input / textarea 内的 ⌘A 仍走原生
  全选文字行为（macOS 用户 muscle memory）

### Cheatsheet modal

- 任务列表段加 `["⌘A", "全选 visible 进 multi-select（再按清空）"]`

## Key design decisions

- **tagName 守卫之后（不抢 input 内 ⌘A）**：macOS / mac OS / Linux 用
  户在 input / textarea 中按 ⌘A 选全文字是核心 muscle memory — 抢走
  这个 binding 会破坏基础体验。仅在 panel 空白区 / row 上响应作"全选
  visible"。
- **toggle 行为 (再按清空)**：第二次 ⌘A 应该 deselect 不应继续保持
  全选 — 与 macOS Finder / 大多数文件管理器同 pattern。比独立 Esc
  清空更直觉。
- **setSelected updater 内对比 prev**：避免 stale state — useCallback
  依赖 visibleTasks 而非 selected，让 callback 引用稳定但内部能拿到最
  新 selected。
- **toast 反馈用 setBulkResultMsg**：与既有"已复制 N 条标题"等批量
  action 同 channel，让 owner 视觉 calibration 一致。2s 短反馈避免 toast
  堆积。
- **空 visibleTasks 短路**：避免触发 toast"⌘A 全选 0 条"噪音；与
  handleCopyVisibleTitles 同防御。
- **不引入 unit test**：与既有 ⌘D / ⌘L / ⌘E / ⌘R alias shortcuts
  同型 plain-modifier keyboard binding；jsdom keyboard event mock 维
  护成本高。通过 vite build + 真实交互验证。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.22s)
