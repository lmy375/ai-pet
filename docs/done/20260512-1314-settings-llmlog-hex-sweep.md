# PanelSettings + LlmLogView hex 收尾（UI 美化 迭代 14）

## 背景

迭代 13 已扫掉 4 个小 panel 文件。剩 PanelSettings (44 hex) + LlmLogView (42 hex) 两个大文件 —— 本轮收尾。

## 改动

### PanelSettings.tsx（~35 hex → token）

通用：
- `#94a3b8` / `#64748b` → `--pet-color-muted`
- `#dc2626` / `#ef4444` / `#b91c1c` → `--pet-tint-red-fg`
- `#fef2f2` / `#fee2e2` → `--pet-tint-red-bg`
- `#fef3c7` → `--pet-tint-yellow-bg`
- `#92400e` → `--pet-tint-yellow-fg`
- `#dcfce7` → `--pet-tint-green-bg`
- `#166534` / `#16a34a` / `#22c55e` → `--pet-tint-green-fg`
- `#f1f5f9` → `--pet-color-bg`
- `#8b5cf6` (reconnect 紫按钮) → `--pet-tint-purple-fg`
- `#0ea5e9` (telegram reconnect) → `--pet-color-accent`
- mcpCard `borderColor: #fca5a5` (错误态) → `color-mix(<tint-red-fg> 40%, transparent)`

保留：
- 各处 `"#fff"` = 彩底按钮上的白字（deliberate）
- accent picker 5 个 swatch (`#0ea5e9` / `#10b981` / `#8b5cf6` / `#f97316` / `#f43f5e`) = **必须 hardcoded** —— 这是给用户看"每个 accent 长什么样"的样本色，理论上不能 theme 跟随，否则切了 accent 后所有 swatch 都变成同一个颜色。

### LlmLogView.tsx（~36 hex → token）

通用：
- slate 系（94a3b8 / 64748b / 475569 / 334155 / 1e293b / e2e8f0 / f1f5f9 / f8fafc）→ token
- `background: "#fff"` / `: "#fff",`（chip inactive bg） → `--pet-color-card`
- `borderBottom: 1px solid #e2e8f0` → `var(--pet-color-border)`

按 role 着色：
- system: `#f0fdf4` / `#16a34a` → tint-green
- user: `#e0f2fe` / `#0284c7` → tint-blue
- assistant: `#faf5ff` / `#9333ea` → tint-purple
- tool: `#fff7ed` / `#ea580c` → tint-orange

特殊：
- `#bae6fd` → `color-mix(<tint-blue-fg> 35%, transparent)`（model chip 活动边框）
- `#86efac` → `color-mix(<tint-green-fg> 40%, transparent)`（copied curl 边框）
- model chip badge `#e0f2fe / #0284c7` → tint-blue
- duration chip `#f0fdf4 / #16a34a` → tint-green
- error chip `#fefce8 / #ca8a04` → tint-yellow
- assistant tag `#faf5ff / #9333ea` → tint-purple
- tool tag `#fff7ed / #ea580c` → tint-orange

## 验收

- LlmLogView 完全无 hardcoded hex 残留。
- PanelSettings 仅剩 8 处 `"#fff"`（彩底白字）+ 5 处 accent swatch hex（deliberate 样本色）。
- 浅 / 深主题切换：所有 LLM log 行的 role tag、status badge、model chip、tool chip 都跟随。
- `npx tsc --noEmit` 通过。

## 完成

- [x] PanelSettings ~35 hex → token
- [x] LlmLogView ~36 hex → token
- [x] 移到 docs/done/
