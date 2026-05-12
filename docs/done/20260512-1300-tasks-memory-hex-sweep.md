# PanelTasks + PanelMemory hex 大扫除（UI 美化 迭代 12）

## 背景

两个面板有 60+ hardcoded hex 散布在 s.* style 字典 + inline 渲染分支里（status badge / due chip / action btn / bulk bar / detail box / history palette / schedule kind 配色等）。多数 light 主题工作正常，dark 主题下大量"白底深字"或"浅粉/浅蓝底"在深底上崩。

## 改动

### PanelTasks.tsx — bulk replace 22 种 hex → token

slate / muted / fg：
- `#94a3b8` / `#475569` / `#64748b` → `var(--pet-color-muted)`
- `#334155` / `#1e293b` → `var(--pet-color-fg)`
- `#f1f5f9` / `#f8fafc` → `var(--pet-color-bg)`
- `#e2e8f0` / `#cbd5e1` → `var(--pet-color-border)`
- `background: "#fff"` → `var(--pet-color-card)`（保留 `color: "#fff"` 用于彩底按钮文字）

red：
- `#fef2f2` / `#fee2e2` → `var(--pet-tint-red-bg)`
- `#991b1b` / `#b91c1c` / `#ef4444` / `#dc2626` → `var(--pet-tint-red-fg)`

orange：
- `#fff7ed` → `var(--pet-tint-orange-bg)`
- `#9a3412` / `#fb923c` → `var(--pet-tint-orange-fg)`

yellow：
- `#fef3c7` → `var(--pet-tint-yellow-bg)`
- `#92400e` / `#a16207` / `#b45309` → `var(--pet-tint-yellow-fg)`

green：
- `#dcfce7` → `var(--pet-tint-green-bg)`
- `#22c55e` / `#16a34a` / `#166534` → `var(--pet-tint-green-fg)`

blue / sky / indigo：
- `#eff6ff` / `#dbeafe` / `#f0f9ff` / `#c7d2fe` → `var(--pet-tint-blue-bg)`
- `#1e3a8a` / `#1e40af` / `#0369a1` / `#3730a3` → `var(--pet-tint-blue-fg)`
- `#0ea5e9` → `var(--pet-color-accent)`

due chip 5-tuple（overdue / today / scheduled-future）：
- `bgActive` 用 `color-mix(<tint-fg> 30%, <tint-bg>)`
- `border` 用 `color-mix(<tint-fg> 40%, transparent)`
- borderActive 用 `tint-fg` 实色

detail copy btn hover：
- `color: #0ea5e9` → `var(--pet-color-accent)`
- `border-color: #7dd3fc` → `color-mix(<accent> 50%, transparent)`

Active checkbox border 中间蓝：
- `#93c5fd` → `color-mix(<accent> 50%, <border>)`

### PanelMemory.tsx — bulk replace 18 种 hex → token

同步 PanelTasks 的所有 slate / red / orange / yellow / green / blue 映射。

特殊：
- `#8b5cf6` (consolidate 按钮 violet-500) → `var(--pet-tint-purple-fg)`
- `#0d9488` (history action create 蓝绿) → `var(--pet-tint-green-fg)`
- btnDanger 边框 `#fecaca` → `color-mix(<tint-red-fg> 40%, transparent)`
- msg 边框 `#bbf7d0` → `color-mix(<tint-green-fg> 35%, transparent)`

## 保留（与 PanelDebug 同思路）

仍剩 9 处 `"#fff"` —— 全部是"彩色按钮文字"语义（accent / tint-red / purple bg 上的白字）。这些是 deliberate 白字以保对比度，不需要 token 化。

## 验收

- dark 主题下：任务行 due chip / status badge / priority badge / action button / bulk bar / detail box / history action chip / schedule kind chip 全部正确跟随。
- 浅主题视觉与原版基本一致（tint vars light 值与原 hex 接近，bgActive / border 用 color-mix 近似还原）。
- `npx tsc --noEmit` 通过。

## 完成

- [x] PanelTasks ~30 处 hex 全量 token 化
- [x] PanelMemory ~25 处 hex 全量 token 化
- [x] due chip palette / detail box / consolidate 按钮等特殊位
- [x] 移到 docs/done/
