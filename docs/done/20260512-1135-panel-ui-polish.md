# Panel UI 美化迭代 1：全局视觉抛光

## 背景

TODO.md【用户确认】「UI 太太太丑了（主要是 Panel 的各个页），修改的美观一些」是大块需求。一次性"全部重做"风险高、blast radius 大。这次先做一个**全局**抛光 —— 通过 `PanelApp.tsx` 注入的 base CSS 让所有 panel 页同步受益，不动具体 panel 组件结构。后续可继续单页深入。

## 改动范围

仅扩展 `PanelApp.tsx` 顶部 `<style>` 块。零业务逻辑改动、零文件新增。

## 目标改进

1. **Tab bar 抛光**：active 指示器换成 inset gradient + 更柔的 underline；inactive hover 走 tint blue 而非纯 bg；focus-visible outline 用 accent rgba。
2. **按钮 hover 反馈**：除现有 color transition 外，给 default state 加一道 `transform: translateY(-0.5px)` + 轻 shadow，让"可点击"更明显；active 时按下回落，模拟真实质感。
3. **输入控件 focus 圈**：现有 box-shadow 单层；改为双层（外层 22% accent halo + 内层 1px 全亮 border），手感更"软"。
4. **Card 容器 default 阴影**：很多 panel 内 div 用 `var(--pet-color-card)` 当卡片但缺 shadow，看着平板。给 `.pet-card`（utility class，opt-in）一个 `--pet-shadow-sm` token 风格的 box-shadow。同时定义 `--pet-shadow-md` 给浮窗 / modal 用。
5. **滚动条**：保留现 hover-加深；inactive 时再淡一些，与背景更融合；轨道留出 2px padding 避免顶到边缘。
6. **selection 高亮**：用 accent rgba 做文本选区背景，与既有 focus 圈呼应（默认浏览器蓝在很多主题里割裂）。
7. **细节**：disabled 按钮统一 0.55 opacity + not-allowed cursor；`::placeholder` 用 muted 色（部分浏览器默认是较深灰，跟 muted token 不一致）。

不动 inline style；不重写已有 `.pet-*` 规则；transition 已存在的不覆盖。

## 不做

- 不一次性重做 5 个 panel 的内部布局 / 配色 —— 太大、易翻车。
- 不引入新依赖（Tailwind / CSS module）—— 维持现有"inline + 顶层 style 注入"。
- 不写测试 —— 视觉变更人工肉眼验。

## 验收

- 浅色 / 深色主题下肉眼看 panel：tab 指示器更顺、按钮 hover 有"浮起来"感、输入框 focus 圈双层柔和。
- `npx tsc --noEmit` 通过。
- 没有现有功能损坏（无 inline 样式被覆盖）。

## 完成

- [x] PanelApp.tsx 顶部 style 块扩展
- [x] 移到 docs/done/
