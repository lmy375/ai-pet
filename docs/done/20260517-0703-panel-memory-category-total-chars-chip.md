# PanelMemory 段 header「📊 总字数」chip（iter #262）

## Background

PanelMemory section header 已有 cat.items.length 计数 badge 与 butler_tasks
专属的 silent / snooze chip，但 owner 想"掂量某类目的累积规模" — 这类目下
detail.md 总字数 + 描述字数加一起，决定要不要 consolidate / 归档 — 没现成
入口。当前只能展开类目逐条 hover 看 "detail X 字" 指示自己加。

本迭代在 items 计数 badge 之后内联 "📊 N k 字" chip，仅当 cat 总字 > 1000
时显（< 1k 字是噪音；owner 能从 item 数判断）。tooltip 拆分描述 / detail
两类让 owner 知道"是描述多还是 detail 多"。

## Changes

仅 `src/components/panel/PanelMemory.tsx`：

- 在 section header 的 `cat.items.length` badge 之后插一个 IIFE 计算 +
  渲染：
  - `descChars`：对 `cat.items` 累加 `Array.from(it.description).length`
    （unicode code-point 数，与 detail.md size 同 char 单位）
  - `detailChars`：累加 `detailSizes[it.detail_path] ?? 0`（既有 useMemo
    map）
  - total < 1000 → 返回 null（不渲染）
  - 否则显 `📊 X.X k 字` 或 `📊 NN k 字`（≥10k 时去小数，紧凑）
  - tooltip：`本段共 N 字（描述 M + detail.md K）· 帮你掂量 consolidate
    时机`

## Key design decisions

- **门槛 1000 字**：避免小类目（user_profile 几条 200 字事实条目）也显 chip
  污染视觉。1k 字以上才有 consolidate / 归档 决策价值。
- **格式分级**：≥10k 时整数（`12 k`），< 10k 时 1 位小数（`3.4 k`）— 避免
  "1.0 k" 这种冗余 decimal。
- **复用既有 detailSizes**：useMemo `Record<detail_path, char_count>` 已实
  现并在 `🚀 外部打开 / 📋 复制 detail.md 全文` 等按钮 gating 复用 — 不需
  要新 IPC。
- **位置紧贴 items 数 badge**：两个都是"段规模"维度，相邻让 owner 一眼看
  count + size 双维度。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.20s)
