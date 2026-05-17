# detail.md 编辑器「🔗」link 按钮 pre-select url placeholder（iter #350）

## Background

detail.md markdown toolbar 已有「🔗」按钮（既有 iter ship）— click 调
`insertMarkdownAtCursor("wrap", "[", "](url)")` 把选区包成
`[selection](url)`。但有选区时光标落在 `)` 之后 — owner 必须手动选
`url` 这 3 个字符再敲键替换地址。3 步键序（click → 选 url → 输入）
不流畅。

本迭代加专用 `insertLinkAtCursor` callback — 有选区时自动 pre-select
`url` 占位符让 owner 立即敲键替换（与 Notion / VS Code markdown ⌘K
链接同 UX，1 步：click → 输入新 url）。空选区行为不变（光标落 `[|]`
让 owner 先敲 link text）。

## Changes

仅 `src/components/panel/PanelTasks.tsx`：

- 新 callback `insertLinkAtCursor`：
  - 算 `urlStart = start + 1 + selected.length + 2`（跳过 `[`、selection、
    `](`），`urlEnd = urlStart + 3`（`url` 3 chars）
  - 走 setEditingDetailContent + rAF
  - 有选区 → `cur.selectionStart = urlStart; cur.selectionEnd = urlEnd`
    pre-select `url`
  - 空选区 → `cur.selectionStart = cur.selectionEnd = start + 1`
    让光标落 `[|]`（owner 先敲 link text；url 仍是 literal placeholder
    可视提示但不 pre-select 避免误覆盖）
  - 同步 setDetailCursorPos / setDetailSelectionEnd 让 status bar
    显新位置正确
- 既有「🔗」按钮 onClick 从 `insertMarkdownAtCursor("wrap", "[",
  "](url)")` 改为 `insertLinkAtCursor`
- tooltip 文案更新说明 pre-select 行为

## Key design decisions

- **仅有选区时 pre-select**：空选区时 owner 显式想"先敲文字再补 url"，
  自动选 url 反而打断流；有选区时 owner 已经"指定了 link text" —
  下一步 99% 是填 url，pre-select 自然。
- **复用 wrap 算法而非 insertMarkdownAtCursor**：insertMarkdownAtCursor
  通用 helper 不该挂 selection-aware "pre-select N chars" 逻辑（与
  既有 bold / code-block / blockquote 不同语义）。专用 helper 让逻辑
  scoped 不污染。
- **`url` literal 占位**：业界惯用 — Notion / Obsidian / GitHub 都
  这样。owner 敲键替换的 muscle memory 直接可用。
- **rAF 设 selection**：与既有 detail editor 所有 markdown 操作同
  pattern — React state update 后 textarea value 重渲，rAF 等到下一帧
  才能正确 setSelection。
- **不引入 unit test**：纯 textarea selection 操作 + 既有
  insertMarkdownAtCursor wrap 算法已被覆盖；jsdom textarea selection
  mock 维护成本高。通过 vite build + 真实交互验证。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.26s)
