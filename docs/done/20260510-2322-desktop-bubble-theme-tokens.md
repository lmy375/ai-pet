# 桌面气泡走主题色 token

## 需求

GOAL.md 写了"做深色 / 浅色主题"。Panel 系列已经全部走 `var(--pet-color-*)`，但桌面宠物窗（ChatMini / ChatPanel / App.tsx 的 MoodWidget + 收起按钮）还是 light-mode 时代留下的硬编码 `rgba(255,255,255,*)` / `#333` / `#475569`。深色模式启用时这些位置变成"白底白窗"或"黑字黑底"，没法读。

## 实现

把 4 处硬编码替换成 design token：

### `src/components/ChatMini.tsx`

- mini chat 滚动容器：`rgba(255,255,255,0.92)` / `#bae6fd` / `#333` → `var(--pet-color-card)` / `var(--pet-color-border)` / `var(--pet-color-fg)`
- 最大化按钮 ⛶：3 处 hardcoded → `var(--pet-color-card)` / `var(--pet-color-border)` / `var(--pet-color-muted)`
- 👍 like-btn CSS 类：基色 `#94a3b8` → `var(--pet-color-muted)`；hover pink `#ec4899` 保留（粉色 like 反馈语义稳定，不主题化）
- 👍 wrapper background `rgba(255,255,255,0.85)` → `var(--pet-color-card)`
- 跳到底浮标 ↓：`#7dd3fc` / `rgba(255,255,255,0.95)` / `#0ea5e9` → 全部 `var(--pet-color-accent)` 或 `card`

### `src/components/ChatPanel.tsx`

- textarea：`rgba(200,200,200,0.5)` / `rgba(255,255,255,0.9)` / `#333` → `var(--pet-color-border)` / `card` / `fg`

### `src/App.tsx`

- MoodWidget badge：`rgba(255,255,255,0.85)` / `rgba(148,163,184,0.4)` / `#475569` → `card` / `border` / `muted`
- 收起按钮 ▶|：同套替换；mouseOver / mouseOut 简化为只切 opacity（不再切 background hex），一行 transition 也只剩 opacity

## 验证

- `npx tsc --noEmit` clean
- 视觉：light 模式下与之前完全一致（token 的 light 值精确对齐 hex）；dark 模式下 mini chat、收起按钮、心情 badge、textarea 全部显深色卡片底 + 浅文字

## 不在本轮范围

- 粉色 like hover 不主题化 —— 跨模式语义稳定
- Live2D 左侧 tab indicator 的渐变色 + 角标红 `#dc2626` 是品牌视觉，跨模式保留
