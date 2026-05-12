# Tab bar pill 指示器 + session 列表 hover（UI 美化 迭代 7）

## 背景

- Tab bar active 指示器原是"整条 2px 下边线"—— 经典但偏功能 / 不够现代；浮窗 IM / 桌面应用普遍换成"居中圆角短条"。
- Session 列表 hover bg 用 `rgba(0,0,0,0.04)` —— light 主题刚好，dark 主题下完全不可见。

## 改动

### Tab bar 指示器（`PanelApp.tsx`）

- Inline 端：`borderBottom` 改成永远 `2px solid transparent` 保布局；视觉指示由 CSS pseudo 接管。
- CSS 端：
  - `.pet-panel-tab[data-active="true"]::after` —— 居中 28px × 3px 圆角短条 + accent halo glow（与迭代 1 shadow 语言呼应）
  - `.pet-panel-tab:hover:not([data-active])::after` —— 浅一档 + 16px 宽，"预告下一步"视觉连续

### Session 列表 hover（`PanelChat.tsx`）

- `background: rgba(0,0,0,0.04)` → `color-mix(<accent> 8%, transparent)`（accent 暖底，theme-aware）
- 加 `border-left: 3px solid transparent` 默认槽位；hover 时变 accent 55% alpha —— 长列表扫读时 hover 落点立刻定位
- transition 扩到 `background-color + box-shadow + border-color`，与任务卡 / 记忆条目同节奏

## 不做

- 不动 session 行 selected 高亮 `#f0f9ff`（inline 硬编码）—— 改它会与 active row 的视觉锚混淆，下次再单独抽 tint 蓝。
- 不写测试。

## 验收

- 切 tab：active 指示器是居中圆角短条 + accent halo；inactive hover 时短条预浮起。
- session 列表 hover：左侧出现 accent 边条 + 暖底 hover bg；浅 / 深主题均可见。
- `npx tsc --noEmit` 通过。

## 完成

- [x] PanelApp tab bar 指示器
- [x] PanelChat session 行 hover
- [x] 移到 docs/done/
