# KeyboardHelpOverlay + ToolCallBlock 抛光（UI 美化 迭代 22）

## 背景

两个使用频次中等但视觉风格陈旧的组件：
- **KeyboardHelpOverlay**（按 ? 唤起）—— 自实现 modal + 自定 key chip 样式，与新 Modal/SectionTitle 节奏割裂
- **ToolCallBlock**（chat 里 inline 渲染工具调用）—— hardcoded slate 边框 / bg / 文字色

## 改动

### `KeyboardHelpOverlay.tsx` 重写

- 整个 overlay → `<Modal maxWidth={560} zIndex={2500}>`：自动获得 backdrop / Esc / pop-in 动画 / shadow-lg
- 每个 group 标题用 `<SectionTitle subtitle={scope}>` —— accent 圆点 + 副标题同基线，与设置页节奏一致
- `KEY_CHIP_STYLE` 升级到 **Mac 风键帽**：
  - padding 2/8（更宽）
  - `borderRadius: 5`
  - 双层 shadow：`inset 0 -1px 0 <border 60%>` 模拟键帽边缘高光 + `0 1px 1px <fg 6%>` 模拟键帽厚度
  - `fontWeight: 500 + letterSpacing: 0.2`
  - bg 改 `card`（旧是 `bg`），让 chip 略浮起
- 行间距：item gap 4→6，行内 keys/desc gap 8→10，minWidth 110→130（容纳更长的组合键）
- 删掉自实现的 Esc useEffect、`if (!visible) return null`、整个 backdrop 模板 —— Modal 全包

### `ToolCallBlock.tsx` token 化

- 容器 border `#e2e8f0` / bg `#f8fafc` → `var(--pet-color-border)` / `var(--pet-color-bg)`
- 新增 `boxShadow: var(--pet-shadow-sm)` 让 inline tool block 与气泡阴影一致
- header padding 8/12 → 10/14，gap 6→8
- header 文字 `#475569` → `var(--pet-color-muted)`
- 工具名 accent `#0ea5e9` → `var(--pet-color-accent)`
- 运行态 / 附件 chip 文字 `#94a3b8` → `var(--pet-color-muted)`
- 内部分割线 `#e2e8f0` → token
- "参数" / "返回值" 副标签加 `letterSpacing: 0.2`
- ▶ 折叠箭头加 `display: inline-block`（修复 rotate 在 inline 元素上不生效的隐患）

### 保留

工具调用 pre 代码块的 `#1e293b` 深蓝底 + `#e2e8f0` 浅字 + `#a7f3d0` 绿结果 —— 终端式 code 块美学，**主动选择跨主题保持深底**（与 PanelDebugLogs 终端块同思路，控制台美学一致性）。

## 验收

- 按 ? 唤起帮助：overlay 走 Modal pop-in 动画；group 标题有 accent 圆点；按键 chip 像 Mac 键帽（边缘高光 + 厚度阴影）。
- chat 里 tool call inline block：border / bg 浅深主题自动跟随；header 间距更舒展；inside 参数 / 返回值 code 块仍深底（intentional）。
- `npx tsc --noEmit` 通过。

## 完成

- [x] KeyboardHelpOverlay 重写（Modal + SectionTitle + mac-style chip）
- [x] ToolCallBlock token 化 + 节奏抛光
- [x] 移到 docs/done/
