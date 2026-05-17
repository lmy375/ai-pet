# detail.md 编辑器 ⌘\` 代码块 markdown shortcut（iter #416）

## Background

detail.md 编辑器已支持 ⌘B 加粗 / ⌘I 斜体 / ⌘K 链接 / ⌘/ 注释等
IDE 风 markdown shortcut（iter #391 / #402 等），但缺**代码块**
shortcut。owner 想包代码 / 命令 / 数据片段时只能：
- 手敲 \`\`\` 前后各两行
- 或先打字再用 toolbar 按钮（toolbar 没代码块按钮）

本 iter 加 ⌘\` 一键 wrap 选区为 fenced code block。

注：与 iter #404 ChatMini 的 ⌘\` transient_note popover 不冲突 —
两者在不同 Tauri webview window（Panel vs ChatMini），各自的 keydown
listener 仅在自己窗口焦点内生效；macOS 系统 ⌘\` 「下一窗口」在
webview 焦点内不冒泡。

## Changes

### `src/components/panel/PanelTasks.tsx`

#### 1. `handleDetailCodeBlock` 处理器

```ts
const handleDetailCodeBlock = useCallback(
  (e: React.KeyboardEvent<HTMLTextAreaElement>): boolean => {
    if (!(e.metaKey || e.ctrlKey)) return false;
    if (e.shiftKey || e.altKey) return false;
    if ((e.nativeEvent as KeyboardEvent).isComposing) return false;
    if (e.key !== "`") return false;
    e.preventDefault();
    insertMarkdownAtCursor("wrap", "\n```\n", "\n```\n");
    return true;
  },
  [insertMarkdownAtCursor],
);
```

复用 `insertMarkdownAtCursor("wrap", prefix, suffix)` 既有 helper：
- 空选 → `\n\`\`\`\n` + cursor + `\n\`\`\`\n`，owner 可立即敲代码内容
- 非空选 → `\n\`\`\`\n<selected>\n\`\`\`\n`，cursor 落 fenced block 之后

modifier guard 与 ⌘B / ⌘I 同模板（拒 shift / alt / composing）。

#### 2. textarea onKeyDown 接入

紧贴 `handleDetailBoldItalic` 之后：

```tsx
if (handleDetailBoldItalic(e)) return;
// ⌘` 代码块：选区 wrap ```\n<sel>\n``` fenced block。与 ⌘B/⌘I 同 wrap-mode 模板。
if (handleDetailCodeBlock(e)) return;
```

按顺序短路 — 优先级与 ⌘B/I 同级，先于 ⌘⇧L link popover。

## Key design decisions

- **fence 前后各 `\n`**：保 \`\`\` 单独成行；markdown 解析器要求
  fence 在 own line（行首），否则当文本不开 code block。前导 `\n`
  在文件首 / 行首场景多一个空行 — 可读性 OK，远好于不加 fence
  渲不出代码块
- **不区分单行 vs 多行选区**：单行也用 fenced block（虽然语义上
  inline `\`text\`` 更紧凑）— owner 想要 inline 可手动加 backtick。
  统一规则让 shortcut 行为可预测
- **不引语言提示符**：fence 后不预填 `\`\`\`python` 等 hint。owner
  自己加；过度推测语言反而误导
- **不与 ChatMini ⌘\` 冲突**：两者跨 webview window，各自 listener
  scope；浏览器 / 系统不会跨窗口 dispatch keydown
- **不引用单 fn unit test**：⌘B/⌘I 既有覆盖；本 fn 是 mirror；build
  pass + 手测足够（验：选段 → ⌘\` → 看 fenced block 形成；空选 →
  ⌘\` → cursor 落中间空行 → 敲代码 → fence 包住）

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.30s)
- 后端无改动
