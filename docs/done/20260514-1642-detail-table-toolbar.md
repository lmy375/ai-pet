# detail.md 工具栏 📊 表格按钮

## 背景

TODO 上 auto-proposed 一条："任务详情 markdown 工具栏新增「📊 表格」按钮：插 3×3 GFM table 骨架；与既有 6 按钮风格一致。"

detail.md 渲染层早已支持 GFM 表格（`inlineMarkdown.tsx` 第 239 行起的 block 解析把 `|...|` + `|---|---|` 段渲染成 `<table>`），但用户要手敲表头 + 分隔行 + 数据行的 markdown 字面挺折磨 —— 4 行 / 9 个 `|` / 9 个空格的占位文本，记忆负担大。表格本来就是结构化记录最常用的载体（任务清单、对比、todo 状态汇总），按钮一键插骨架后只需 5 秒补 cell 内容即可形成可读视图。

## 改动

### `src/components/panel/PanelTasks.tsx`

#### 新 helper

```ts
const insertTableSkeletonAtCursor = useCallback(() => {
  const ta = detailEditorRef.current;
  if (!ta) return;
  const start = ta.selectionStart ?? 0;
  const end = ta.selectionEnd ?? start;
  const value = ta.value;
  const needLeadingNL = start > 0 && value[start - 1] !== "\n";
  const lead = needLeadingNL ? "\n" : "";
  const skeleton =
    `${lead}| 列 1 | 列 2 | 列 3 |\n| --- | --- | --- |\n|  |  |  |\n|  |  |  |\n`;
  const next = value.slice(0, start) + skeleton + value.slice(end);
  const headerCellStart = start + lead.length + 2;
  const headerCellEnd = headerCellStart + 3;
  setEditingDetailContent(next);
  requestAnimationFrame(() => {
    const cur = detailEditorRef.current;
    if (!cur) return;
    cur.focus();
    cur.selectionStart = headerCellStart;
    cur.selectionEnd = headerCellEnd;
  });
}, []);
```

#### 按钮

工具栏末尾、紧贴 ❝ 引用块按钮：

```tsx
<button
  type="button"
  onClick={insertTableSkeletonAtCursor}
  title="表格（3×3 GFM）。插入 | 列 1 | 列 2 | 列 3 | + 分隔行 + 2 空白数据行；光标自动选中『列 1』，直接敲即覆盖。需独占整段，按钮会自动补换行。"
  style={mdToolbarBtnStyle}
>
  📊
</button>
```

## 关键设计

- **不扩 `insertMarkdownAtCursor` 第三个 mode**：既有 wrap / line-prefix 两个 mode 都是"在 selection 上做局部变形"；表格是"块级模板 + 落点为内部 selection"，跟那俩语义正交。强行扩第三 mode 会让接口与单元测试都复杂。新写一个 useCallback 干净、行内可读。
- **3×3 默认**：太小（2×2）放不下太多数据，太大（5×5）骨架占屏 + 多数 cell 留空浪费。3×3 是"最小可用对比表"的 sweet spot；后续要扩列 / 行 markdown 复制粘贴一行就行。
- **占位文 "列 1"**：可读 + 中文用户一眼知道这是表头 + 长度刚好够 select 改写。比 `Col 1` 一致性更强（detail.md 默认中文叙事多）。
- **`needLeadingNL` 自动补换行**：用户从某段行尾按按钮时 cursor 不在行首，骨架直接拼上去会被前文 "吞" 进同段（markdown 表格必须独占段落）。补一个 `\n` 是最便宜的"段落起点"修复。
- **光标自动选中"列 1"**：用 select 而不是 cursor —— 让用户 immediate 进入"敲就替换"模式，省一次"双击全选第一格"动作。这与 GitHub PR 描述里"输入框 placeholder 选中"的 UX 模式一致。
- **GFM table 已在 parseMarkdown 第 239 行处理**：渲染层无需改动，骨架在阅读 / split / preview 模式下立刻可视；数据 cell 留空时 `<td>` 也是空，与"还没填" 的语义一致。

## 不做

- **不支持自定义行 / 列数**：要在按钮上加 `nxm` 弹窗 / hover 配置面板太重；用户复制粘贴扩展 markdown 行本来就快。
- **不动 create-task 表单**：detail.md 编辑器只在"已存在任务"的 detail 编辑器里，create 表单走 description 字段不挂表格按钮。
- **不写测试**：纯 DOM textarea 操作 + 字符串拼接，逻辑 50 行；vitest jsdom 下 textarea selection / requestAnimationFrame 行为与真 webview 不完全一致，单测意义有限。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.19s
- 改动 ~50 行（helper 30 + 按钮 9 + comment）；既有 6 按钮 / textarea / preview 模式不变。

## TODO 状态

5 条候选 auto-proposed 已完成 2 条，余 3 条留池：
- 任务详情图片懒加载
- 任务行 hover detail 预览
- pinned 任务过滤 chip

## 后续

- 工具栏按钮命中"行内 token" 自动检测（如光标在 `**` 内时 B 键拆掉粗体而非再包一层）。
- markdown 表格按 `Tab` 跳下一格 / `Shift+Tab` 反向：textarea 原生 Tab 缩进，要 hook 键盘事件 + 解析 markdown 表格行位置；复杂度较高，留待用户真有诉求再做。
- 自定义 cell 默认占位文（如 "—" / 空 / 用户最近输入）。
