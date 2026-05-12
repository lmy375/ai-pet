# Marks modal token 对齐 + 全局 modal keyframes（UI 美化 迭代 25）

## 背景

PanelChat 的 marks modal（📌 全部标记消息查看）是个复杂 modal：sticky header + 内嵌 search input + copy 按钮 + 滚动 entries 列表 + maxHeight 75vh。它的 layout 自带很多约束，迁移到通用 `<Modal>` 会反受约束。但它的 overlay backdrop / shadow / 圆角仍是 hardcoded 风格，与其它 dialog 视觉割裂。

同时，Modal.tsx 自带 `pet-modal-{fade-in,pop}` keyframes inline inject —— Modal 没挂载时这些动画名不存在，inline modal（marks modal）想复用就找不到定义。

## 改动

### marks modal（PanelChat.tsx）token 对齐

- backdrop `rgba(15, 23, 42, 0.55)` → `color-mix(<fg> 50%, transparent)`（与 Modal 同主题感知）
- card `borderRadius: 10` → 12（与 Modal 同节奏）
- card `boxShadow: "0 20px 60px rgba(0, 0, 0, 0.35)"` → `var(--pet-shadow-lg)`
- 加 fade-in + pop-in 动画（用全局 keyframes）

### `src/styles/app.css` 加全局 modal keyframes

- `@keyframes pet-modal-fade-in`（opacity 0→1）
- `@keyframes pet-modal-pop`（scale 0.96 + translateY 8px → 1, 0）
- `@media (prefers-reduced-motion: reduce) { .pet-modal-card { animation: none } }`

### Modal.tsx 去重

- 删掉组件 inline 的 keyframes block（已在 app.css）
- 留注释指引

## 验收

- 打开 chat 顶部 📌 标记列表 modal：有 fade-in + pop-in 动画；backdrop / shadow 与其它 dialog 一致。
- 其它已迁移的 Modal dialog 行为不变（keyframes 在全局 + 组件 inline 双定义合法，后者只是被前者代替）。
- `npx tsc --noEmit` 通过。

## 完成

- [x] marks modal token + 动画对齐
- [x] keyframes 提到 app.css 全局
- [x] Modal.tsx 去重
- [x] 移到 docs/done/
