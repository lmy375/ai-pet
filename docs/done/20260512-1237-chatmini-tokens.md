# ChatMini 视觉对齐 + shadow token 提升到全局（UI 美化 迭代 9）

## 背景

ChatMini 是桌面宠物窗口的内嵌聊天气泡（独立 webview）。迭代 1 注入的 `--pet-shadow-{sm,md,lg}` 之前只在 PanelApp.tsx 的 inline `<style>` 里 —— 仅 panel 窗口可见。桌面宠物窗口（App.tsx）/ 调试窗口（DebugApp.tsx）拿不到 token，沿用 `rgba(0,0,0,0.x)` 硬阴影：light 主题刚好；dark 主题下深底叠加几乎不可见。

## 改动

### Shadow token 提升到全局 `src/styles/app.css`

把 PanelApp.tsx inline 的 `:root` + `[data-theme="dark"]` 阴影块整体搬到 `app.css`。所有 webview 都通过 `import "./styles/app.css"`（main.tsx 顶部）拿到 token，跨窗口共享。

### `ChatMini.tsx` hex / shadow → token

| 位置 | 旧 | 新 |
|------|----|----|
| `.pet-mini-maxbtn:hover` shadow | `0 2px 6px rgba(0,0,0,0.18)` | `var(--pet-shadow-md)` |
| 搜索框容器 shadow | `0 2px 6px rgba(0,0,0,0.12)` | `var(--pet-shadow-sm)` |
| 搜索无命中文字色 | `#dc2626` | `var(--pet-tint-red-fg)` |
| NOW 浮窗 shadow | `0 4px 16px rgba(0,0,0,0.18)` | `var(--pet-shadow-md)` |
| 📋 复制按钮 shadow | `0 1px 3px rgba(0,0,0,0.1)` | `var(--pet-shadow-sm)` |
| 复制 popover shadow | `0 2px 8px rgba(0,0,0,0.15)` | `var(--pet-shadow-md)` |
| ⛶ 最大化按钮 shadow | `0 1px 3px rgba(0,0,0,0.1)` | `var(--pet-shadow-sm)` |
| Mini chat 容器 shadow | `0 2px 8px rgba(0,0,0,0.08)` | `var(--pet-shadow-md)` |
| 已复制 ✓ fg | `#16a34a` | `var(--pet-tint-green-fg)` |
| Active search hit outline | `#f59e0b` + rgba(245,158,11,0.25) | `tint-orange-fg` + 28% color-mix |
| Inactive search hit dashed | `#fbbf24` | `tint-orange-fg` 60% color-mix |
| ↓ scroll-to-bottom shadow | `0 2px 6px rgba(0,0,0,0.15)` | `var(--pet-shadow-md)` |
| Copy toast bg | `rgba(22,163,74,0.92)` / `rgba(220,38,38,0.92)` | `tint-green-fg` / `tint-red-fg` 92% color-mix |
| Copy toast shadow | `0 2px 8px rgba(0,0,0,0.2)` | `var(--pet-shadow-md)` |

保留 hardcoded：
- `#ec4899` like 按钮 hover（原 comment 明确"粉色 like 反馈不主题化"，跨主题语义稳定）
- `#fff` toast 文本（白字在彩色底上始终可读）

### PanelApp.tsx 去重

删掉 inline `<style>` 内的 `:root` + `[data-theme="dark"]` 阴影块（已搬到 app.css）。

## 不做

- 不动 ChatMini 自身的 `.pet-mini-chat::-webkit-scrollbar`（局部规则，更精确）。
- 不动 like 按钮粉色 / fff 白字（与上文一致的理由）。
- 不写测试。

## 验收

- 桌面宠物窗口浅 / 深主题切换：聊天气泡阴影、各按钮阴影、search 命中高亮、toast 配色都跟随。
- 搜索无命中时的红色提示文字、复制 ✓ 绿、active hit 橙、错误 toast 红 —— 全部走 tint 体系，与 panel 各页面色域统一。
- `npx tsc --noEmit` 通过。

## 完成

- [x] app.css 加 shadow tokens :root + [data-theme="dark"]
- [x] PanelApp.tsx 去重 inline shadow tokens
- [x] ChatMini.tsx hex/shadow 全量迁移
- [x] 移到 docs/done/
