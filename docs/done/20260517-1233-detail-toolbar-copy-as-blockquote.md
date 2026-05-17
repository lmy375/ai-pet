# detail.md 编辑器 toolbar「📋❝ 复制选区为引用块」按钮（iter #292）

## Background

owner 写 detail.md 笔记后常想"把这一段 quote 出去发同事 / 贴别处"。当前
路径：textarea 内选中文字 → ⌘C 复制 → 粘到目标 → 手动在每行前加 `> `
让其成 markdown blockquote。多步且容易漏行。

本迭代加 📋❝ toolbar 按钮：选中文字 → 一键拼成 `> 行 1\n> 行 2\n…`
blockquote 写剪贴板。**不动** detail 自身（与既有 ❝ 按钮区别 — 那是直接
在 textarea 里写 `>` 前缀）。

## Changes

仅 `src/components/panel/PanelTasks.tsx`：

- **`copySelectionAsBlockquote` useCallback**：
  - 读 textarea selectionStart/End；空选区 → friendly toast"📋 选中文字后再
    点 — 没有选区可复制为 blockquote"
  - selection split `\n`，每行前缀 `> `（空行变 `>` 让多行 blockquote 在外
    部 markdown renderer 内连续不断段）
  - `navigator.clipboard.writeText(quoted)` + 3.5s 反馈 toast 显字 / 行数

- **toolbar 按钮**：在 📜 插日期模板 之后插 📋❝ 按钮（emoji 复合表
  "复制 + 引用" 双语义）

## Key design decisions

- **不动 detail textarea 内容**：与既有 ❝ "在 textarea 里写 `>` 前缀" 互补。
  本按钮是 export 路径 — 拿到外部用；❝ 是 in-place 编辑路径。
- **空行变 `>` 而非完全删除**：让多行选区 quote 后在外部 renderer 仍是
  连续 blockquote，不被空白行打断分成两段。
- **字 + 行数双反馈**：让 owner 知道"我刚 quote 了多大一段"，避免误以为
  没复制 / 复制错。
- **emoji 复合 📋❝**：与既有 single-emoji toolbar 风格略不同（B / • /
  🔗 / 📅 / 📜 / 「」/ 🔢 等），但 📋 = clipboard / ❝ = quote 二者合
  表"复制为 quote" 语义自文档。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.21s)
