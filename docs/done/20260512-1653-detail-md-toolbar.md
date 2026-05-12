# task detail.md 编辑器 markdown 工具栏

## 背景

迭代 21 已抛光 detail.md textarea 视觉（padding / radius / shadow）。但内容编辑仍需用户手敲 markdown：写"加粗"得敲 `**...**`、列表得敲 `- `、链接得敲 `[](url)`。三个高频动作可以一键。

## 改动

`src-tauri` 无改动；纯前端：`src/components/panel/PanelTasks.tsx`。

### 新 helper `insertMarkdownAtCursor`

支持两种 mode：
- **wrap**：选区前后包 prefix / suffix（粗体、链接）。空选区 → 插入空 wrapper，光标落在中间。
- **line-prefix**：每选中行行首加 prefix（列表、引用）。空选区 → 给当前行加。

实现细节：
- 通过 `detailEditorRef.current` 拿 textarea
- 计算 `selectionStart` / `selectionEnd`，slice 拼新字符串
- `setEditingDetailContent(next)` 后用 `requestAnimationFrame` 等下一帧再设光标位置 + focus（与既有 `insertImageBlobsIntoDetail` 同模式，避免 React 渲染覆盖）

### 工具栏 3 按钮

渲染在 detailViewMode `!== "preview"` 时（edit 或 split 模式有 textarea 才显），位置在 textarea 上方：
- **B**（粗体）—— wrap `**...**`，title 解释语法 + 空选区时光标位置
- **•**（列表）—— line-prefix `- `，title 提示每行行首
- **🔗**（链接）—— wrap `[...](url)`，title 解释占位

### 样式

新增 `mdToolbarBtnStyle` 常量：3/8 padding，token border + card bg + fg color；全局 button rule（迭代 1）自动给 hover lift + shadow。

## 不做

- 不加 H1/H2/H3 标题按钮：detail.md 在 panel 显示走 `parseMarkdown`，标题在长文里少用，列表是更高频形态。
- 不加 strike / italic / code block：3 按钮覆盖 80%，更多按钮反而稀释 visual 焦点；用户能直接敲 `~~text~~` / `*x*` / ` ``` ` 等。
- 不加快捷键（⌘B / ⌘I 等）：textarea 默认快捷键由浏览器接管，自定义会与 OS 习惯冲突；用户更愿意点鼠标 / 直接敲符号。

## 验收

- 切到「任务」tab → 展开任意任务 → 点 ✏️ 进入 detail.md 编辑
- textarea 上方出现三个按钮
- 点 **B**：选中"加粗"两字 → 变 `**加粗**`；无选区时 → 插入 `**|**`，光标落中间
- 点 **•**：选中 3 行 → 行首各加 `- `
- 点 **🔗**：选中"GitHub" → 变 `[GitHub](url)`
- preview 模式（无 textarea）不渲染工具栏

`npx tsc --noEmit` 通过。

## 完成

- [x] insertMarkdownAtCursor helper（wrap + line-prefix 双模式）
- [x] 3 按钮 + 工具栏 JSX
- [x] mdToolbarBtnStyle 常量
- [x] TODO.md 移除该行
- [x] 移到 docs/done/
