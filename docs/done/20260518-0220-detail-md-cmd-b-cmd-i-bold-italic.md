# detail.md 编辑器 ⌘B 加粗 / ⌘I 斜体 快捷键（iter #349）

## Background

detail.md textarea 已有 markdown toolbar 含「B」加粗按钮 + 链接 / 列表 /
table 等模板按钮。但缺标准 ⌘B / ⌘I 快捷键 — 与所有现代 markdown 编辑
器（Notion / Obsidian / VS Code / Bear）肌肉记忆冲突 — owner 习惯 ⌘B
直接套 `**`。

本迭代加 ⌘B / ⌘I 快捷键复用既有 `insertMarkdownAtCursor("wrap")` 算
法 — 与既有 toolbar 加粗按钮同 backend。

## Changes

仅 `src/components/panel/PanelTasks.tsx`：

- 新 callback `handleDetailBoldItalic`:
  - 命中 `(metaKey || ctrlKey)` + 无 shift / alt + 非 IME composing
  - key=='b' → `insertMarkdownAtCursor("wrap", "**", "**")`
  - key=='i' → `insertMarkdownAtCursor("wrap", "*", "*")`
  - 其它 key 返 false 让 handler chain 继续
- 两 textarea onKeyDown 块（edit + split mode）都接入：
  - `if (handleDetailBoldItalic(e)) return;`
  - 位置：⌘⇧K 删除行之后 / ⌘S 保存之前（IDE 行操作 → markdown wrap
    → 保存 三层分明）
- placeholder 文案补 `⌘B 加粗 / ⌘I 斜体` 让 owner 发现快捷键
- ⌘/ cheatsheet modal detail editor 段加新条：
  `["⌘B / ⌘I", "加粗 / 斜体（选区 wrap **/*；空选时插模板）"]`

## Key design decisions

- **复用既有 `insertMarkdownAtCursor("wrap", ...)`**：与既有 toolbar 加
  粗按钮同后端 — 行为一致（空选时插 `**` `**` 模板光标落中间 / 有选
  时 wrap）。不引第二份算法避免漂移。
- **⌘B / ⌘I 标准映射**：全 markdown 编辑器 / 富文本编辑器（Word / Pages
  / Docs / Notion / Obsidian）的 ⌘B = bold / ⌘I = italic 肌肉记忆。
  不做反映射。
- **无 shift / alt 守卫**：保留 ⌘⇧B / ⌘⌥B 等组合给未来扩展（如 ⌘⇧B
  转换 H1 / ⌘⌥B 转 underline 等）— modifier cluster 严格不重叠。
- **IME composing 守卫**：中文输入法（拼音 / 倍速）输入态按 b / i 可能
  落在 composing 中间 — 此时 ⌘B 应让 IME 处理（如改 candidate）而非
  抢 markdown wrap。与既有 bracket pair / duplicate line / list-continue
  同 guard pattern。
- **handler chain 位置**：⌘⇧K 之后（IDE 行操作集群结束）→ ⌘B/⌘I（markdown
  wrap）→ ⌘S（保存）→ ⌘⇧Enter / ⌘⌥Enter（保存变体）。modifier
  cluster 按"动作类型"分层而非"频率"。
- **不引入 unit test**：纯 keyboard event → existing helper 路径；既有
  `insertMarkdownAtCursor` 也未单测；通过 vite build + 真实交互验证。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.25s)
