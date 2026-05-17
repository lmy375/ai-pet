# PanelMemory item action row「✏️」rename mini-button（iter #351）

## Background

PanelMemory item 已有"双击 title inline rename"行为（plain 双击）+ ⌘
+双击 进编辑 modal（iter #321）。但双击 affordance 对鼠标党 / 触屏党 /
发现 double-click 困难的用户不够直观 — 想改名得"试试双击"才知道有这功
能。

本迭代加 action row「✏️」mini-button，与既有双击行为同 backend，让
鼠标 click affordance 显式 surface 改名入口。

## Changes

仅 `src/components/panel/PanelMemory.tsx`：

- item action row 在 既有 📋📄 复制 detail 路径按钮之后 / 🏷 改类目 按钮
  之前插「✏️」rename button：
  - onClick 调 `setRenamingMemoryKey(${catKey}::${item.title})` +
    `setRenameMemoryDraft(item.title)` — 与既有双击 title 的 plain
    分支同 setState。inline rename input 会因 state 切换自动展示。
  - renameKey 与 title IIFE 内同算法（`${catKey}::${title}`）— inline
    重算避免跨 IIFE scope 借用
  - tooltip 含完整 UX 提示（Enter 提交 / Esc 取消）

## Key design decisions

- **同 backend，仅添 affordance**：rename button 不引入新逻辑路径 —
  既有 inline rename input + commit / cancel handlers (Enter / blur /
  Esc) 全套已 wired。鼠标点 button 与双击 title 走完全同一 state 切换，
  行为一致。
- **位置紧贴 📋📄**：rename 与 path-copy 都是 "item-level 元数据" 类操
  作（不像 🏷 改类目 / 🔖 加 tag 是修改内容）。adjacency 让 owner 视
  觉建立"📋📄 ✏️ 🏷 🔖" 渐进式分组。
- **renameKey 内联重算**：定义在 title-rendering IIFE 中的 renameKey
  不能跨 IIFE 借用；在 button onClick 内重算同公式 `${catKey}::${item.
  title}` — 单行内联 + 注释说明同源算法。
- **emoji ✏️ 而非 🖋 / 📝**：与"改 / 写"动作的视觉默认绑定；📝 已被
  「📝 复制本条整段 markdown」占用，避免视觉混淆。
- **不引入 unit test**：纯 state 切换 + 既有 inline rename 路径已 wired
  through 双击入口；button 是平行 affordance 走同一 setState。通过 vite
  build + 真实交互验证。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.25s)
