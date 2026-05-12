# 桌面宠物 App + Live2D + ChatPanel hex/shadow 收尾（UI 美化 迭代 15）

## 背景

迭代 9 把 ChatMini 接进 token 系统，但桌面宠物其它入口（App.tsx 顶层 / Live2DCharacter / ChatPanel 输入框）仍有 hardcoded hex + rgba 阴影散落。本轮一次性扫掉，同步 PanelChat 内的相同 sky-blue rgba。

## 改动

### `Live2DCharacter.tsx`

- `#e53935 / #888` 状态文字色 → `var(--pet-tint-red-fg) / var(--pet-color-muted)`

### `App.tsx`

- 隐藏 pet 期间未读 badge `#dc2626` → `var(--pet-tint-red-fg)`
- badge 白色描边 `#fff` → `var(--pet-color-card)`（dark 主题下白边在 accent 红上太刺，用 card 色融入）
- badge `boxShadow: 0 1px 3px rgba(0,0,0,0.2)` → `var(--pet-shadow-sm)`
- drop-toast `boxShadow: 0 4px 12px rgba(0,0,0,0.25)` → `var(--pet-shadow-md)`
- 其它 `boxShadow: 0 1px 3px rgba(0,0,0,0.08)` (2 处) → `var(--pet-shadow-sm)`
- `boxShadow: 0 1px 3px rgba(0,0,0,0.12)` → `var(--pet-shadow-sm)`

### `ChatPanel.tsx`

- focus 样式：`#38bdf8` border + `rgba(56,189,248,0.18)` halo → accent CSS var + color-mix
- 拖拽态高亮 bg `rgba(56,189,248,0.22)` → `color-mix(<accent> 22%, transparent)`

### `PanelChat.tsx`（同步同类 sky-blue 残留 + shadow rgba）

- 拖拽高亮 bg `rgba(56,189,248,0.18)` → accent color-mix
- 输入 focus border `#38bdf8` + halo → accent CSS var
- copy btn hover border `#7dd3fc` → accent 50% color-mix
- 4 处 `boxShadow: 0 4px 12px rgba(0,0,0,0.18)` → `var(--pet-shadow-md)`
- 2 处 `boxShadow: 0 2px 8px rgba(0,0,0,0.2)` → `var(--pet-shadow-md)`
- `boxShadow: 0 4px 12px rgba(0,0,0,0.08)` → `var(--pet-shadow-md)`
- `boxShadow: 0 1px 4px rgba(0,0,0,0.08)` → `var(--pet-shadow-sm)`

## 保留

- `rgba(15,23,42,0.78)` 桌面 attachments ✕ 按钮 dark overlay — deliberate（图片右上角"删除"小钮，与背景反差需要稳定 dark 色）。
- ChatPanel attachments thumbnail 上的 `color: "#fff"` — 白字在 dark overlay 上。

## 验收

- 桌面宠物窗口 dark 主题：未读 badge 红色背景跟随 token；drop toast 阴影适配；拖拽高亮和 focus 圈用 accent。
- `npx tsc --noEmit` 通过。

## 完成

- [x] Live2DCharacter / App / ChatPanel hex+shadow → token
- [x] PanelChat 同类 sky-blue rgba + 7 处 shadow → token
- [x] 移到 docs/done/
