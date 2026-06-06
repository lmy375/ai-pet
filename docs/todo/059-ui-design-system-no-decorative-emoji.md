# 059 · UI 生产级整改 — 统一设计语言、emoji 仅作内容不作组件

ChatMini / PanelChat / PanelMemory / PanelTasks 多处截图证据：emoji 被滥用为 UI 组件（toolbar chip 标识 "📋 / 🌐 / 📊"、hover action icon "🚀 / 📁 / 📜"、cat header 装饰、空状态大图 📭、stats prefix "📚2 条"），导致整体观感 Demo / 山寨。GOAL.md「UI 要美观可爱」+「以生产级别要求」实质被违反。

需求：
- 引入统一 icon set（lucide / heroicons / phosphor 三选一，落地时定），全项目 UI 组件（button / chip / hover action / empty state / stats prefix）替换 emoji 为该 set SVG icon。
- emoji **仅保留**于：pet utterance 内容文案、mood tag 语义符号（047 figure badge）、user 自己输入的字符、chat bubble 文本里 — 这些属于"内容"，不属于"组件"。
- 设计 token 集中：新建 `src/styles/tokens.ts`（或 CSS 变量）定义 色板 / 间距阶 / 字号阶 / 圆角阶 / shadow / icon size，禁止 inline 硬编码 `#xxx`、`12px` 等。
- 所有现有组件 inline 硬编码样式扫一遍迁到 token；CI / lint 加规则禁止 raw color literal / raw px（白名单仅 1px borders / animation tweaks）。
- 046 / 051 / 058 declutter 系列落地时按此规范执行：不是简单删 emoji chip，而是替换为 icon set 组件后再决定是否收进 ⋯。
- 047 mood badge 属"内容"侧，本需求不动；但其位置与样式需走 design token（不是 fixed `bottom: 16px`）。
- 不引入主题切换（GOAL.md 第 16 行禁），但 token 抽象本身留给未来扩展空间不强制。
