# 小 panel 文件 hex 扫尾（UI 美化 迭代 13）

## 背景

主面板四件（Chat / Tasks / Memory / Persona）+ Debug + DebugApp 已 token 化。剩下散在 chip / stats card / bits / debug logs 等小组件里。本轮一次性扫掉。

## 改动

### `panelChatBits.tsx`（5 hex → token）

- `#fef3c7` / `#92400e` → tint-yellow（搜索关键词高亮 mark）
- `#16a34a` → tint-green（reaction liked accent）
- `#ca8a04` → tint-yellow（reaction puzzled accent）
- `#dc2626` → tint-red（reaction disliked accent）

### `PanelDebugLogs.tsx`（2 hex → token）

- `#475569` → muted（"全部"chip accent + INFO level chip）
- `#dc2626` → tint-red（ERROR level chip）

保留终端块内（`#0f172a` 背景 / `#e2e8f0` text / `#f87171` ERROR line / `#fbbf24` WARN line 等）—— 终端式 log 区是 deliberate 的"控制台风"，跨主题保持深底。

### `PanelChipStrip.tsx`（13 hex → token）

slate / 多色 chip → tint：
- `#94a3b8` / `#64748b` → `--pet-color-muted`
- `#cbd5e1` → `--pet-color-border`
- `#fff` (chip base bg) → `--pet-color-card`
- `#f8fafc` → `--pet-color-bg`
- `#0ea5e9` → `--pet-color-accent`
- `#a855f7` / `#7c3aed` / `#5b21b6` → `--pet-tint-purple-fg`
- `#ea580c` → `--pet-tint-orange-fg`
- `#dc2626` / `#991b1b` → `--pet-tint-red-fg`
- `#16a34a` / `#15803d` → `--pet-tint-green-fg`
- `#0891b2` → `--pet-tint-blue-fg`

### `PanelStatsCard.tsx`（10 hex → token）

5 个数字 stat 各自的 accent（today/week/lifetime/since-last/companionship）：
- `#94a3b8` / `#64748b` / `#475569` → `--pet-color-muted`
- `#0ea5e9` → `--pet-color-accent`
- `#ea580c` / `#d97706` / `#fff7ed` → `--pet-tint-orange-*`
- `#6366f1` → `--pet-tint-blue-fg`
- `#7c3aed` → `--pet-tint-purple-fg`
- `#0d9488` → `--pet-tint-green-fg`

## 保留

各文件剩余 1 处 `"#fff"` —— 都是彩色背景按钮上的白字，deliberate 保对比度。
PanelDebugLogs 终端区整套 hardcoded 颜色 —— 控制台美学一致性。

## 验收

- 浅 / 深主题下 chip 区 / 反馈按钮 / stat 大数字均跟随；终端块保持深色不变。
- `npx tsc --noEmit` 通过。

## 完成

- [x] panelChatBits / PanelDebugLogs / PanelChipStrip / PanelStatsCard 4 个文件 ~30 hex 全量 token 化
- [x] 移到 docs/done/
