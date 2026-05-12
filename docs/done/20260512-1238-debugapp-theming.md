# DebugApp 接入主题系统 + tab bar pill（UI 美化 迭代 10）

## 背景

DebugApp 是调试窗口（独立 webview，通过 `调试 ↗` 按钮打开）。之前完全没接主题：

- 根容器 `background: "#f8fafc"` 硬编码 → dark 主题下白底刺眼。
- Tab bar 用 `#fff`/`#e2e8f0`/`#0ea5e9`/`#64748b` hardcoded → 无 theme 跟随。
- 永远 light 外观，与 panel 切主题后视觉割裂。

## 改动

`DebugApp.tsx` 重写：

### 主题入口

- 模块顶部 `applyTheme(getStoredTheme(), getStoredAccent())` 启动即应用（与 PanelApp / App 入口同模式，避免 light flash）。
- 加 useEffect 监听 `theme-change` / `accent-change` Tauri 事件 —— 用户在 panel 切主题时 debug 窗口跟着变（与 App.tsx 桌面宠物同模式）。
- 引入 `setStoredTheme` / `setStoredAccent` 在收到事件时也写本地存储，确保下次开窗用最新值。

### Tab bar 视觉对齐 PanelApp

- 根 bg：`#f8fafc` → `var(--pet-color-bg)`
- Tab bar bg：`#fff` → `var(--pet-color-card)`
- Tab bar border：`#e2e8f0` → `var(--pet-color-border)`
- Inactive tab color：`#64748b` → `var(--pet-color-muted)`
- Active tab color：`#0ea5e9` → `var(--pet-color-accent)`
- Active indicator：从"整条 2px 下边线"换成 CSS `::after` 居中圆角短条 + accent halo（与 PanelApp 迭代 7 同款）。
- Inactive hover：加 accent 8% alpha 暖底 + 16px 短条 ::after"预告"，节奏完全一致。
- 移除原 `transition: all 0.2s`，改成精确字段 transition。

## 不做

- 不动 PanelDebug / LlmLogView / PanelDebugStats / PanelDebugLogs 内部 —— scope 限根容器 + tab bar。这些子组件已大量用 CSS var（如 var(--pet-color-card) 等）— 之前外层 hardcoded 反而是不一致点。
- 不写测试 —— 纯 UI。

## 验收

- 在 panel 切深色主题 → debug 窗口自动跟随（背景 / tab bar / 内部子组件全套）。
- 浅 / 深下 tab 指示器都是圆角短条 + halo，与 panel 一致。
- Inactive hover 时有暖底 + 16px 预告短条。
- `npx tsc --noEmit` 通过。

## 完成

- [x] DebugApp.tsx 主题接入
- [x] tab bar token 化 + pill 指示器
- [x] 移到 docs/done/
