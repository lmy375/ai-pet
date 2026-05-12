# TaskProposalCard + 两个下拉菜单抛光（UI 美化 迭代 23）

## 背景

聊天里 inline 渲染的卡片 / 下拉是用户高频接触的视觉元素，但都还有 hardcoded 残留：
- **TaskProposalCard**：用 indigo 系硬编码（`#c7d2fe / #eef2ff / #4338ca / #6366f1 / #1e1b4b ...`）+ inline gradient，dark 主题下完全不可读
- **SlashCommandMenu / ImagePromptHistoryMenu**：hover bg `rgba(0,0,0,0.04)` dark 不可见；shadow 用 hardcoded rgba

## 改动

### TaskProposalCard.tsx

全量 token 化 + pill 化 badge：
- card：indigo gradient → `var(--pet-tint-purple-bg)` + 35% alpha border + `var(--pet-shadow-sm)`，padding 12→14/16，radius 8→10
- head：`#4338ca` → `var(--pet-tint-purple-fg)` + letterSpacing 0.2
- title / body：fg / muted token；body lineHeight 1.5→1.55
- priBadge：`#fef3c7/#92400e` → tint-yellow + pill (radius 999) + weight 600 + letterSpacing + 18% alpha border
- dueBadge：`#e0e7ff/#3730a3` → tint-blue + 同 pill 升级
- btnPrimary：`#6366f1/#fff` → `tint-purple-fg/#fff` + weight 600 + letterSpacing 0.2
- btnSecondary：`#c7d2fe/#fff/#4338ca` → 35% alpha tint-purple-fg border + card bg + tint-purple-fg
- btnDisabled / status / err 文字全部走 token

### SlashCommandMenu.tsx

- hover bg：`rgba(0,0,0,0.04)` → `color-mix(<accent> 8%)`（与 session list / tab bar 同 hover 节奏）
- shadow：hardcoded → `var(--pet-shadow-md)`

### ImagePromptHistoryMenu.tsx

同步 SlashCommandMenu 的 hover bg + shadow 升级。

## 验收

- 聊天里 LLM 弹出 task proposal card：紫底 + 紫边 + pill 化 priority/due chip + accent halo button；浅 / 深主题都顺。
- 输入 `/` 或 `/image ` 弹下拉菜单：hover 走 accent 暖底（与 session list 一致），shadow 立体。
- `npx tsc --noEmit` 通过。

## 完成

- [x] TaskProposalCard 全量 token + pill badge
- [x] SlashCommandMenu hover + shadow
- [x] ImagePromptHistoryMenu hover + shadow
- [x] 移到 docs/done/
