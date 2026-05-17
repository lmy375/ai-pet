# detail.md 编辑器「📐 字数目标」chip（iter #273）

## Background

owner 用 detail.md 写日记 / 写作打卡 / 长篇笔记时常想"今天定个 N 字目标"
做进度提示。现有 status bar 只显当前字数 + > 2000/5000 阈值色 banner，但
没"目标 / 进度"维度。

本迭代加 per-task 字数目标 chip：未设时显"📐 设目标"按钮；设了显
"📐 N/M" + 三档配色（< 30% 红 / 30-90% amber / 90-150% green / > 150%
muted overshoot）。localStorage 持久化 per-title 跨重启保留。

## Changes

仅 `src/components/panel/PanelTasks.tsx`：

- **state**：
  - `wordCountGoal: number | null` — 当前编辑 task 的目标
  - `editingGoal: boolean` — inline input 显隐
  - `goalDraft: string` — input 当前值
  - `detailGoalKey(title)` helper → `pet-detail-goal-${title}`
  - useEffect 监听 `editingDetailTitle` 切换 → 从 localStorage sync goal
  - `persistWordCountGoal` useCallback → write / remove localStorage

- **render**：在 status bar 字数 chip 之后插 IIFE：
  - editingGoal === true → number input（autoFocus / Enter commit / Esc 取消
    / blur 保存）。stopPropagation 避免 ⌘S / Esc 等全局 hotkey 抢键
  - wordCountGoal === null → "📐 设目标" dashed-border 按钮 click 进入编辑
  - 已设 → "📐 N/M" 彩色 chip：
    - ratio < 0.3 → red tint（远未到）
    - 0.3-0.9 → amber tint（差一截）
    - 0.9-1.5 → green tint（达标 ✓）
    - > 1.5 → muted（超量提示，避免过度膨胀）
  - chip 双击 → 编辑目标；右键 → 清除目标

## Key design decisions

- **per-task localStorage 而非全局**：不同 task 目标差异大（300 字日记 vs
  2000 字技术文档）；title 是 key，rename 时 stale 不删（罕见 + 不害）。
- **三档色 + overshoot 第四档**：< 30% / 30-90% / 90-150% / > 150%，让 owner
  在写作过程中获得"刚起步 / 接近 / 达标 / 已超量"四个粒度的反馈，比单纯
  百分比更直觉。overshoot muted 是温和提示"够了，别水了"。
- **Enter / Esc / blur 三入口同 commit**：减少习惯学习 — 用户按任意一个
  "完成动作"键都能保存或取消。input keydown stopPropagation 防全局 ⌘S /
  Esc 抢键。
- **0 / 空输入 → 清除目标**：让 owner 用 "📐 设目标 → 0 → Enter" 显式撤销
  目标，比强制 require positive 友好。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.27s)
