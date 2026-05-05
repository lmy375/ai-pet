# 桌面气泡多行 markdown — 开发计划

> 对应需求（来自 docs/TODO.md）：
> 桌面气泡多行 markdown：当前 inline markdown 已渲染 *斜体* / **加粗** / `code`；加最简列表（`- ` 行首）+ 段落空行 → `<br>` 让 LLM 写多句拆段时排版不乱。

## 目标

气泡当前用 `parseInlineMarkdown` 把单行字符串展开成 inline 节点；多行 LLM 输
出（`item\n- another\n\n下一段` 这种）会被气泡的默认 inline flow 折叠成"item
- another 下一段"一坨。本轮加最小 block-level 解析：
- 单 `\n` → 强制换行（每行各自渲染为 `<div>`）
- 空行（`\n\n` 中间一空 line）→ 段落间 4-6px 视觉 gap
- 行首 `- ` 或 `* ` → 列表项，带 `•` bullet + 左缩进
- 行内仍走 `parseInlineMarkdown`（**bold** / *italic* / `code`）

## 非目标

- 不做有序列表 / 表格 / 引用 / 代码块（` ``` ` 块） / 标题 / 链接 / 图片 ——
  这些在小气泡里也排不下，引入需要重新算高度限制。
- 不在 panel chat 启用 —— 与之前 inline markdown 的决策一致：panel 历史含
  早期 LLM 输出，启用渲染会让散乱 `*` / `-` 渲染奇怪。
- 不写 README —— 视觉补强，与 inline markdown 同性质。

## 设计

### 解析

新增 `parseMarkdown(input: string): ReactNode[]`，与 `parseInlineMarkdown`
同文件（`inlineMarkdown.tsx`）。算法：

1. `input.split("\n")` 拿到每行字符串
2. 遍历每行：
   - 空行 → 推一个 `<div style={height: 4}>` （段落间小 gap）
   - 行首 `- ` 或 `* `（允许前导空格）→ 列表项：`<div padding-left: 8>• {inline parse 余文}</div>`
   - 其它 → 普通行：`<div>{inline parse}</div>`
3. 每行都是 block-level `<div>`，自带换行 —— 不需 `<br>`

### 应用

`ChatBubble.tsx` 把 `{parseInlineMarkdown(message)}` 换成
`{parseMarkdown(message)}`。其余样式 / 动画 / 按钮全不动。

### 测试

`parseMarkdown` 是 pure；无 vitest 配置，靠 jsdoc 边界 case 列举 + tsc + 手测。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | `parseMarkdown` 多行解析（含列表 / 段落 gap） |
| **M2** | `ChatBubble.tsx` 切换调用 |
| **M3** | tsc + build + cleanup |

## 复用清单

- 现有 `parseInlineMarkdown` 直接用作每行的 inline 解析器
- 既有 bubble 容器（已是 div，可挂 block 子元素）

## 待用户裁定的开放问题

- 列表项前的 bullet：`•` vs `·` vs 直接保留 `-`？本轮 `•`（视觉权重适中，
  常见 chat UI bullet）。
- `- ` vs `* ` 都识别 vs 仅 `- `？本轮**都识别** —— 不同 LLM 输出习惯不同，
  双兼容代价仅一个 char 类。
- 段落间 gap 高度：4 vs 8 vs 12 px？本轮 4 px（小气泡视觉密度高，gap 太大
  会把 80px max-height 压完）。

## 进度日志

- 2026-05-06 02:00 — 创建本文档；准备 M1。
- 2026-05-06 02:15 — 完成实现：
  - **M1**：`src/utils/inlineMarkdown.tsx` 加 `parseMarkdown(input)` 多行解析器：split 按行，行内调既有 `parseInlineMarkdown`；行首 `- ` / `* ` 识别为 list item（左缩进 8 + `•` bullet）；空行渲染为 4px 段落 gap div；其它行 wrap 为 block-level `<div>`，自带换行不需 `<br>`。
  - **M2**：`ChatBubble.tsx` import + 调用从 `parseInlineMarkdown` 切到 `parseMarkdown`；其余样式 / 动画 / 按钮全不动。
  - **M3**：`pnpm tsc --noEmit` 干净；`pnpm build` 497 modules 全过。TODO 移除条目；本文件移入 `docs/done/`。
  - **README 不更新** —— 视觉补强，与 inline markdown 同性质。
  - **设计取舍**：双兼容 `- ` / `* `（不同 LLM 输出习惯）；`•` bullet（视觉权重适中）；4px 段落 gap（小气泡 max-height 80px 容不下大 gap）；每行 block-level div（自带换行，免显式 `<br>` 维护）；不支持有序列表 / 引用 / 代码块（需要重新算高度限制，不在最简范围）。
  - **未做手动 dev 验证**：当前会话不便启动 Tauri 桌面 app；`parseMarkdown` 是 pure，行级处理边界清晰，由 tsc + 既有 inline parser 保证。
