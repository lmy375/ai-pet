# 051 · PanelChat 瘦身 + GOAL 违规清理

- ✅ part1：dark mode 整套删除（GOAL.md 17 行违规）。`theme.ts` 退化为
  light-only；删 `Theme` type / `getStoredTheme` / `setStoredTheme` /
  ctxMenu 🌙 项 / 顶 tab 🌙 button / `ThemeToggleRow` / `[data-theme="dark"]`
  CSS。accent 5 色保留（非主题切换）。tsc + vite build clean。
- ⏳ 剩：think-fold 修复扩 PanelChat / 顶 tab 瘦身 ≤6 / 气泡 hover 6→3 /
  顶栏 chip 删 📋📅 / suggested replies 过滤 dev artifacts / 底部 selector
  收 ⋯。这些都需 dev server 视觉验证。
