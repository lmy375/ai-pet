# 主题切换跨 webview 同步 + 设置页 toggle 控件

## 需求

TODO 说"当前 applyTheme 在加载时跑一次，用户无 UI 切换"—— grep 后发现 PanelApp
其实有个 🌙/☀️ toggle 按钮在 tab bar 右上角，但藏得深。更严重的问题是：

- toggle 只改 panel webview 的 CSS vars；桌面宠物窗（独立 webview）从不收到
  通知，仍维持加载时的主题
- 设置页里完全没有主题相关入口

修这两个：跨窗同步 + 设置页加显眼 toggle。

## 实现

### 跨窗 emit / listen

主题改了 → emit("theme-change", "light"|"dark") → 其它 webview 监听后 setStoredTheme
+ applyTheme 持久化 + 刷 CSS vars。

`src/PanelApp.tsx`：

- `import { emit, listen } from "@tauri-apps/api/event"`
- `toggleTheme()` 末尾加 `void emit("theme-change", next)`
- useEffect 加 listener：收到 event 时 `setTheme((cur) => { if (cur===next) return cur; applyTheme + setStoredTheme; return next })`，cur===next 守护防回环

`src/App.tsx`（桌面宠物）：

- import setStoredTheme
- useEffect 加 listener：getStoredTheme === next 守护后 setStoredTheme + applyTheme

桌面 pet 只接收，不 emit —— ChatMini / ChatPanel 里没有 toggle UI，避免双向回环。

### PanelSettings 增加显眼 toggle

文件末尾加 `ThemeToggleRow` 函数组件：

- 双键 pill 切换 `☀️ 浅色` / `🌙 深色`，与 PanelChat 搜索 scope 切换 chip 同款样式
- 内部 state 启动时从 localStorage 直读（不和 PanelApp 共享 state，跨组件树）
- 点击 → 动态 import theme module + tauri event module → applyTheme + setStoredTheme + emit
- 一行 muted 文案"切换立即同步到桌面宠物 / 调试窗口"，让用户知道一键变三窗

PanelSettings 顶部新增"外观" SearchableSection（在"本地数据目录"之前）放置
`<ThemeToggleRow />`。

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 设置页 ☀️ → 🌙 → panel + 桌面宠物 + 调试 panel 三窗都同步切深色
  - tab bar 顶部的旧 🌙 按钮仍 work，且 panel 内的 ThemeToggleRow 跟着切（PanelApp 的 setTheme 由 listener 触发）
  - 关掉桌面宠物窗、单独切 panel 主题 → 不报错；下次开桌面宠物从 localStorage 读到最新值

## 不在本轮范围

- 系统主题跟随（macOS prefers-color-scheme）：当前默认 light，不监听系统切换；
  后续可加 `window.matchMedia("(prefers-color-scheme: dark)").addListener(...)`
- 设置页 ThemeToggleRow 用 dynamic import 拉 theme + event 模块：避免循环依
  赖 / startup 加载副作用，仅在用户真点击 toggle 时才解析

## TODO 池剩余

- ChatMini 顶部 📋 弹框加"复制带时间"开关
- PanelTasks 任务卡片拖拽调 priority
- /image -n 局部成功失败混合反馈
- ChatMini 角色 glyph 可配置
