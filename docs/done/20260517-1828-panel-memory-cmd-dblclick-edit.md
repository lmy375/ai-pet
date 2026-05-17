# PanelMemory item ⌘ + 双击 title 进编辑 modal（iter #321）

## Background

PanelMemory item title 现已有"双击改名"inline 行为 — 双击 title 把 div
切换成 input 让 owner 改名（仅 title 维度）。但 owner 想做完整编辑（改
category / description / detail）只能走 action row 的「编辑」按钮 — 三
步：找 item → 点 action row → 点编辑。

本迭代加 ⌘/Ctrl + 双击 title → 直接进编辑 modal 的 shortcut，与既有 plain
双击 = inline 改名共存。两 gesture 互补：plain 双击只改名（轻），
⌘ + 双击 走完整编辑 modal（重）。

## Changes

仅 `src/components/panel/PanelMemory.tsx`：

- item title `div` 的 `onDoubleClick` handler 扩展：
  - 命中 `e.metaKey || e.ctrlKey` → setEditingItem({...}) 进编辑 modal
    （与 action row「编辑」按钮 onClick 完全同 payload）
  - 否则走既有 inline rename 路径（setRenamingMemoryKey + draft）
- tooltip 文案从 "双击改名" → "双击改名 / ⌘ + 双击 进编辑 modal" 让
  owner 发现新 gesture

## Key design decisions

- **⌘ + 双击 而非替换 plain 双击**：既有 plain 双击 = inline 改名 是有
  价值的轻量路径（owner 只想改名时不必弹 modal）。直接替换会破坏既有
  UX。两 gesture 共存 — Mac/Win/Linux 用户都习惯 ⌘/Ctrl 作"显式 / 重操
  作"修饰。
- **同 payload as action row「编辑」按钮**：保后端 / modal 行为一致 —
  owner 进 modal 后看到的状态与点「编辑」按钮无差异。
- **不需要 keyup / mousedown 分流**：onDoubleClick event 本身就有 modifier
  状态，metaKey / ctrlKey 直接可读。无 race condition。
- **不动 inline rename state**：⌘ + 双击 分支 `return` 前不调
  setRenamingMemoryKey/setRenameMemoryDraft，让 modal 与 rename 状态完
  全 disjoint —— owner 不会同时进入两 mode。
- **不引入 unit test**：纯 JSX gesture 行为；DOM dblclick 事件在 jsdom
  下 metaKey 可 mock 但维护成本不值；通过 vite build + 真实交互验证。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.22s)
