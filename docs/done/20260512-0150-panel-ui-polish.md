# Panel UI 通用美化

## 需求

用户反馈"UI 太太太丑了（主要是 Panel 的各个页）"。Panel 几个 tab（聊天 /
任务 / 记忆 / 人格 / 设置）当前长期是默认 system 字体 + 平面 1px 灰边
卡片 + 默认 sans-serif，整体观感粗糙。本轮做一遍"通用抛光"：字体、tab
bar、focus 环、卡片阴影、bubble 形状、scrollbar 等共享视觉元素改一遍，
不动业务逻辑。

## 实现

### `src/PanelApp.tsx`

- 顶层注入 `<style>` 块，覆盖整个 panel 窗口的默认：
  - 字体栈：`-apple-system, "SF Pro Display", "PingFang SC", "Helvetica Neue",
    "Segoe UI Variable", system-ui, sans-serif` + `font-smoothing antialiased`
    +`text-rendering optimizeLegibility`
  - 全局 `div::-webkit-scrollbar`：8px 宽，半透明 slate thumb，hover 加深
  - `input/textarea/select:focus`：去 native outline，accent 色 border
    + `color-mix` 22% 柔光环（box-shadow 3px）—— 跨主题/accent 自动跟随
  - `button` 通用 transition：bg / color / border / transform / shadow
    平滑 ~120ms，没写 transition 的 inline button 也能丝滑 hover
  - `.pet-panel-tab` class：hover 暖底 + focus-visible 键盘 outline，给
    顶部 tab bar 用
- Tab bar 自身改造：
  - 5 个 tab 按钮加 `className="pet-panel-tab"` + `data-active`
  - 字号 14 → 13.5，letter-spacing 0.2，inactive font-weight 400 → 500
  - inactive cursor pointer / active cursor default
  - 右侧 ? / 🌙 / 调试↗ 都用同一个 class，hover 状态统一
- 红色 overdue 角标：阴影色用 #dc2626 透明色，淡出更柔和

### `src/components/panel/PanelSettings.tsx`

- `sectionStyle`：从单 marginBottom 升级为完整卡片（card bg + 1px border +
  10px radius + 微阴影 + 16/18 内边距）
- `sectionTitle`：加底分隔线 + letter-spacing；字号 14 → 13.5

### `src/components/panel/PanelTasks.tsx`

- `formCard`：bg → card + border + 10px radius + 微阴影
- `sectionTitle`：底分隔线 + letter-spacing
- 不动 section 整段（已有 nested formCard / item 卡片，再外包一层会"卡中
  卡"）
- `container` 去 `fontFamily` 内联（让 PanelApp 全局字体接管）

### `src/components/panel/PanelMemory.tsx`

- `s.section`：保持 marginBottom，仅 title 加底分隔线
- `s.item / s.btn / s.btnDanger / s.btnPrimary / s.input / s.textarea`：
  border-radius 4/6 → 6/8，padding 略增（视觉呼吸）
- `s.msg`：加 1px 绿边，更像独立 alert

### `src/components/panel/PanelChat.tsx`

- `sessionBarStyle`：8px padding → 10px，加 box-shadow 0 1px 0 半透明把
  bar 与下面 content 区分得更清晰
- `newSessionBtnStyle`：4px → 5px padding 让按钮高度与 sessionBar 协调

### `src/components/panel/panelChatBits.tsx`

- `bubbleStyle`：圆角 16→18 / 4→6 让 bubble 更现代圆润；assistant bubble
  加 1px 边（之前完全 borderless），与 dark mode card 区别更清；user bubble
  阴影从灰色透明改成 `color-mix` accent 35% 透明，bubble 自带 accent 色光
  晕，跨主题/accent 跟随
- 注：bubble 影响 ChatMini + PanelChat 历史 + 跨会话搜索结果 三处

## 验证

- `npx tsc --noEmit` clean
- 视觉：
  - 顶部 tab bar：active accent 色下划线 + inactive hover 浅灰底切换更平滑
  - 设置页：每段 section 是一张白卡，标题下有细线，整体清爽
  - 任务页：创建表单卡片有阴影；任务行 8px 圆角更现代
  - 记忆页：记忆条目圆角变温和；按钮风格统一
  - 聊天页：bubble 圆角更现代；user bubble 自带 accent 色光晕
  - 全局 scrollbar 不再是默认丑的 macOS 滚动条；hover 加深
  - 所有 input / textarea 聚焦时有 accent 色柔光环
- 字体：从默认 sans-serif 升到 macOS SF Pro + PingFang，中英文都更紧致

## 不在本轮范围

- 没改 PanelPersona / PanelDebug 子组件 inline style：那两个有大量自定义
  布局（mood / risk 卡片群），单独打磨需要单独一轮
- 没引入 Tailwind / CSS-in-JS 库：保持现有 inline + 局部 `<style>` 块的
  混合模式，避免大型迁移
- 没做暗黑模式精细调色：现有 theme tokens 已覆盖；本轮新加的 box-shadow
  在 light 下生效优秀，dark 下半透明 shadow 仍可读
- 没做 accessibility 全面 audit：仅加了 focus-visible outline 让键盘
  可达，aria 标签未系统补全

## TODO 池剩余

- PanelTasks detail.md markdown 预览
- PanelDebug 工具风险 inline 调整
