# 任务详情 markdown 工具栏 +2：代码块 + 待办 checkbox

## 背景

TODO 最后一项（auto-proposed 几轮之前）：

> 任务详情 markdown 工具栏 +2：` ``` ` 代码块 + `- [ ]` todo checkbox 两个按钮（现有 B / • / 🔗 的补完）。

任务 detail.md 是宠物记录"过程笔记"的核心区域：调试日志贴代码、整理思路用 todo 清单、长任务跟踪用 checkbox。现有 3 个按钮（**B** 粗体 / **•** 列表 / **🔗** 链接）覆盖了通用 markdown，但代码与 todo 这两个高频形态还得手敲围栏 / 方括号。补这两个按钮闭合常用场景。

## 改动

### `src/components/panel/PanelTasks.tsx`

工具栏现有 3 个按钮后追加 2 个，复用既有 `insertMarkdownAtCursor` helper —— 不引入新 helper、不动 detailEditorRef 路径。

**1. `</>` 代码块按钮**

```tsx
<button
  onClick={() => insertMarkdownAtCursor("wrap", "```\n", "\n```")}
  title="代码块（```\n...\n```）。选中后点击包裹；无选区时光标落在两道围栏之间让你直接敲。"
  style={{ ...mdToolbarBtnStyle, fontFamily: "'SF Mono', 'Menlo', monospace" }}
>
  {"</>"}
</button>
```

- 用 wrap mode + prefix `\`\`\`\n` / suffix `\n\`\`\`` 形成 fenced code block。
- 选中文本：包裹成代码块。
- 无选区：光标落两道围栏之间（`insertMarkdownAtCursor` 的"空选区 → start + prefix.length"自然行为），用户接着敲。
- glyph 用 `</>` 配 monospace 字体，与"代码"语义直观对齐；比"\`\`\`" 字面量在 UI 更克制（字面量 backtick 在窄按钮里挤）。

**2. `☐` 待办 checkbox 按钮**

```tsx
<button
  onClick={() => insertMarkdownAtCursor("line-prefix", "- [ ] ", "")}
  title="待办（- [ ] ...）。每选中行的行首加 - [ ]。完成后手动改成 - [x] 即标记完成；GitHub / Obsidian / Notion 都识别。"
  style={mdToolbarBtnStyle}
>
  ☐
</button>
```

- line-prefix mode：每个选中行的行首插入 `- [ ] `。
- 多行选区：批量"任务化"一段笔记。
- 单行 / 空选区：当前行加前缀。
- glyph `☐`（U+2610 BALLOT BOX）—— 空 checkbox 一目了然；与勾选后的 `[x]` 视觉对偶（用户改成 `- [x]` 后渲染层 markdown parser 通常会换成 `☑`）。
- 不自动插 `[x]` 版本：UX 上"加 todo" 是创建动作，"勾掉" 是完成动作，要分两个按钮反而冗余；用户手改 `[ ]` → `[x]` 是 1 键操作。

## 不做

- **不在 markdown parser 里渲染 todo 状态**。当前 parseMarkdown 把 `- [ ]` / `- [x]` 当普通 list item 渲；未来加 checkbox UI 可独立做。本轮仅给作者侧便利。
- **不加表格 / 引用 / 标题 等其它按钮**。当前 5 个已 cover 90% 用例；加更多会让 toolbar 横向溢出，需要换布局（折叠菜单 / 二级行），那是独立的设计活。
- **不动既有 3 按钮**。新加的是叠加。
- **不写测试**。前端无 vitest；按钮 onClick 调既有 helper，逻辑完全继承已测过的 textarea selection 路径。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.15s
- 改动 ~30 行（两个按钮 + tooltip + monospace 字体）；既有 B / • / 🔗 / insertMarkdownAtCursor / detailEditorRef 路径全部不动。

## 后续

- 当前 TODO 全空。下次启动将进入 auto-propose 分支提新需求。
- toolbar 视觉规模化（>5 按钮）后考虑折叠菜单或二级行（"加粗 / 斜体 / 高亮" 共"格式"组、"代码 / 引用 / 表格" 共"块"组、"链接 / 图片 / todo" 共"其它"组）。
