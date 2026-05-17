# detail.md 编辑器 toolbar「📜 插日期 + 进度笔记」模板按钮（iter #287）

## Background

长 detail.md（持续写 days / weeks 的进度笔记）owner 习惯按日期分段：
```
## 2026-05-15 进度
...

## 2026-05-16 进度
...
```

现要手敲 `## 2026-05-17 进度` + 两行换行，或走 📅 插当前时间按钮拿
`2026-05-17 10:30` 再手加 `##` + 后缀。步骤多 + 容易格式不一致。

本迭代加 📜 按钮：一键插 `## YYYY-MM-DD 进度\n\n` 块级模板，光标落第三行
让 owner 直接敲今日笔记。

## Changes

仅 `src/components/panel/PanelTasks.tsx`：

- **`insertDateHeadingAtCursor` useCallback**：
  - 拿 textarea selection 范围（start / end）
  - 算"是否需要前导 `\n`"（光标前一字符不是 `\n` 时补一个，让 H2 头独占整段）
  - 拼 `${lead}## ${YYYY-MM-DD} 进度\n\n`
  - replace selection → setEditingDetailContent；cursorPos = start + block.length
    （即 H2 + 空行 + 行首，owner 直接敲就是第三行内容）
  - requestAnimationFrame 后 focus + setSelectionRange

- **toolbar 按钮**：在 📅 插时间 之后插 📜 按钮，hover title 解释模板形态

## Key design decisions

- **块级模板独占段 + 智能补 `\n`**：与既有 `insertTableSkeletonAtCursor`
  同模板 — block-level 模板要求前面是换行；光标前不是 `\n` 时补一个让
  H2 头不被前文吞进同段（"abc## 2026-05-17 进度" 不会被 markdown 渲染成
  H2，必须独占行）。
- **光标落第三行**：H2 + 两个换行 = 第三行。owner 直接敲就是今日笔记内容。
  与 `insertTableSkeletonAtCursor` 把光标落"列 1"模板字让 owner 选删后敲
  类似的"省一步"思路。
- **"进度" 后缀 hardcoded**：让模板风格统一（owner 不会一会儿写"进度"一会
  儿写"日报"），减少决策疲劳。想改风格的 owner 仍可手敲。
- **复用 mdToolbarBtnStyle + emoji**：与既有 8 个工具栏按钮（B / • / 🔗 / </> /
  ☐ / ❝ / 📊 / 📅 / ✓ / 「」/ 🔢）视觉一致。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.19s)
