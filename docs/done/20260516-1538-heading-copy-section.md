# detail.md preview heading 旁 📋 复制本节按钮

## 背景

TODO 上 auto-proposed 一条："detail.md preview hover heading 显『📋 复制 section markdown』小按钮：拷整个 H2 段含子内容到剪贴板。"

既有 detail.md 工具栏的 📋 / 📤 都是**整体**复制（detail.md 全文 / task 完整 markdown）。owner 在长 detail（含多 H2 节）下常想拷"刚做完的本节" 单独贴 share / issue —— 当前要手动从 textarea 选范围 + ⌘C。preview 模式下 heading 旁加一个 📋 小按钮，一键拷该节直到下个同级 / 更高级 heading 之前的全部内容。

## 改动

### `src/utils/inlineMarkdown.tsx`

#### `ParseMarkdownOpts.onHeadingCopySection`

```ts
/// callback 收到 heading 计数（emit 顺序 1-indexed，与 headingIdPrefix 同源），
/// caller 根据 counter 从 raw markdown 提取 section。不传时不渲染按钮（保持
/// heading 简洁，不打扰 chat / mini chat 等其它 callsite）。
onHeadingCopySection?: (counter: number) => void;
```

#### heading 渲染加 flex 容器 + 📋 按钮

```tsx
const copyCb = opts?.onHeadingCopySection;
<div
  id={...}
  style={{
    fontWeight: 600, fontSize, marginTop: 4, marginBottom: 2,
    scrollMarginTop: 12,
    ...(copyCb ? { display: "flex", alignItems: "center", gap: 6 } : {}),
  }}
>
  <span style={copyCb ? { flex: 1, minWidth: 0 } : undefined}>
    {parseInlineMarkdown(body)}
  </span>
  {copyCb && (
    <button
      onClick={(e) => { e.stopPropagation(); copyCb(counter); }}
      title="复制本节 markdown 到剪贴板..."
      style={{
        fontSize: 10, padding: "1px 5px",
        border: "1px solid var(--pet-color-border)",
        borderRadius: 3,
        background: "var(--pet-color-card)",
        color: "var(--pet-color-muted)",
        opacity: 0.5,  // default low-key
        transition: "opacity 120ms ease-out",
      }}
      onMouseEnter={(e) => { e.currentTarget.style.opacity = "1"; }}
      onMouseLeave={(e) => { e.currentTarget.style.opacity = "0.5"; }}
    >
      📋
    </button>
  )}
</div>
```

无 callback 时 heading 结构不变（保持 `<div>{parseInlineMarkdown(body)}</div>`），确保 chat / mini chat / memory hover preview 等其它 callsite 视觉零变化。

### `src/components/panel/PanelTasks.tsx`

#### `extractSectionFromMarkdown` pure helper

```ts
function extractSectionFromMarkdown(md: string, counter: number): string {
  const lines = md.split("\n");
  let seen = 0, startIdx = -1, startLevel = 0;
  for (let i = 0; i < lines.length; i++) {
    const m = lines[i].match(/^(#{1,3})\s+/);
    if (m) {
      seen += 1;
      if (seen === counter) {
        startIdx = i;
        startLevel = m[1].length;
        break;
      }
    }
  }
  if (startIdx < 0) return "";
  let endIdx = lines.length;
  for (let i = startIdx + 1; i < lines.length; i++) {
    const m = lines[i].match(/^(#{1,3})\s+/);
    if (m && m[1].length <= startLevel) {
      endIdx = i;
      break;
    }
  }
  return lines.slice(startIdx, endIdx).join("\n").trimEnd();
}
```

H2 节会包含其下的 H3 子节（同 markdown 结构语义）。H3 节止于下个 H1/H2/H3。

#### `handleCopyHeadingSection` useCallback

```ts
const handleCopyHeadingSection = useCallback(
  (counter: number) => {
    const section = extractSectionFromMarkdown(editingDetailContent, counter);
    if (!section) {
      setBulkResultMsg("未找到节内容");
      // ...
      return;
    }
    void navigator.clipboard
      .writeText(section)
      .then(() => setBulkResultMsg(`已复制本节 markdown（${section.length} 字符）`))
      .catch((e) => setBulkResultMsg(`复制失败：${e}`))
      .finally(() => setTimeout(() => setBulkResultMsg(""), 4000));
  },
  [editingDetailContent],
);
```

#### 两 parseMarkdown 调用接入

split + preview 模式的 parseMarkdown 调用各加 `onHeadingCopySection: handleCopyHeadingSection`。

## 关键设计

- **counter 同源于 parseMarkdown 内部 emit 顺序**：避免文本 slug 化 / 同名 heading 碰撞。`extractSectionFromMarkdown` 也按出现顺序数 N 找起点，与 parseMarkdown 同语义。
- **不破坏既有 heading 结构**：仅当 `opts.onHeadingCopySection` 传入时才切到 flex 容器 + 加按钮。chat / mini chat / memory hover preview 等其它 5+ 个 parseMarkdown callsite 都不传此选项 → heading 渲染完全不变。
- **section 终止条件 `level <= startLevel`**：H2 节包含其下的 H3 子节（markdown 嵌套语义）；遇到下个 H2（同级）或 H1（更高级）即终止。H3 节止于下个 H1/H2/H3（同级或更高）。
- **opacity 0.5 → 1 hover**：按钮默认低 opacity 不抢 heading 视觉焦点；hover 时强化让 owner 知道"这能点"。inline `onMouseEnter` / `onMouseLeave` 避免引 CSS class（parseMarkdown 不挂样式表）。
- **e.stopPropagation()**：防 heading 行 click 事件冒泡到外层 preview pane 容器（避免触发 expand / 别的副作用）。
- **`flex: 1; minWidth: 0`**：让 heading 文本占用剩余宽，长 heading 自然 wrap；按钮固定贴右。`minWidth: 0` 是 flex 容器内子元素能正常 shrink 的 CSS 必要条件。
- **section 拼好后 `.trimEnd()`**：去掉末尾可能的空行 / 换行 —— 拷到 chat / issue 时不带尾部 newline 让 paste 更干净。

## 不做

- **不接 H4-H6**：parseMarkdown 本身只渲 H1-H3。
- **不写测试**：纯字符串 split + 计数 + 切片；既有 inline helpers 都视觉验证。`extractSectionFromMarkdown` 是 pure helper，未来可加单测但当前 callsite 单一。
- **不接 hover-only display: none → flex**：当前 opacity 0.5 → 1 已是渐进显隐 + 物理元素始终存在（不破坏 layout）。display: none 切换会导致 heading 行宽度跳变。
- **不带按钮 fade-in 动画**：opacity transition 120ms 已经够柔和。
- **不在 chat / mini chat 同款接入**：那两处 markdown 渲染场景下 owner 主要是"读消息"非"管理 detail"；加 📋 按钮在每个 heading 旁会让对话视觉杂乱。本 iter 专注 detail.md。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.19s
- 改动 ~120 行（inlineMarkdown opts + heading 渲染 60 + PanelTasks 提取 helper 30 + handleCopyHeadingSection callback 25 + 两 callsite 各 1 行）；既有 parseMarkdown 其它分支 / chat / mini chat / memory hover preview 等多 callsite 完全不动。

## TODO 状态

6 条 auto-proposed 已完成 5 条，余 1 条留池：
- detail.md 大纲浮窗 active heading 高亮

## 后续

- 同款扩到 outline 浮窗：每条 outline item 旁也加 📋（与 heading 旁同 callback，UX 二选一入口）。
- `e.altKey + click` 拷 section + heading parent 上下文一段（如 H3 节 click 时拷 H2 父节摘要 + H3 本身）。
- toast 显被拷的"前 N 字预览" 让 owner 一眼确认是哪节。
