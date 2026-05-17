# detail.md「📋❝ copy as blockquote」按钮 trailing-empty-line 修剪（iter #347）

## Background

TODO 项：「detail.md 选区 toolbar 加「📋 copy as blockquote」：textarea
选区 → button → 复制 `> <text>` 到剪贴板（每行加 `> ` 前缀）」。

审查代码发现该按钮 + helper 已实现：
- `copySelectionAsBlockquote` useCallback in PanelTasks.tsx L2482
- 「📋❝」按钮 in markdown toolbar L10937
- 算法：每行加 `> ` 前缀，空行变 `>` 单字符，clipboard.writeText + 3.5s
  toast

但有一个小 polish 缺：triple-click 行选 / 拖到段末时 selection 常含
trailing `\n` → 输出末行变 `>` stray quote 行（外部 markdown 渲染时多
空 quote）。本 iter 完成 polish 让 trailing empty / whitespace-only
lines 自动剥掉。

## Changes

仅 `src/components/panel/PanelTasks.tsx`：

- `copySelectionAsBlockquote` 算法补：
  - split lines 后从末向前数 trailing 空 / 仅空白行（`line.trim() === ""`）
    计数 → `slice(0, -dropTail)` 截断
  - leading empty 行**保留**（owner 显式选段开头空行可能有意），不动
  - 全空白选区（剥后 `lines.length === 0`）→ 友好 toast "选区仅含空白
    — 没有有效 blockquote 内容"，拒绝复制

## Key design decisions

- **仅剥 trailing，不剥 leading**：trailing empty 是常见的 triple-click /
  拖动末尾"误选"产物；leading empty 是 owner 显式拖动开头特意留的（很
  少误触）。非对称处理更接近 owner 意图。
- **`trim() === ""` 检测**：与 `length === 0` 相比 — 含 spaces / tabs 的
  "空" 行也剥（这种行 quote 后 `> 	` 是奇怪空 quote）。
- **全空白选区直接拒**：不复制空 blockquote 到剪贴板（污染剪贴板 / 让
  owner 后续 paste 时困惑）；toast 提示重新选。
- **既有 button + helper 不动其它逻辑**：单字数 / 多行 toast / 失败兜
  底等保持原状 — polish 仅在算法层加 trim step。
- **不引入 unit test**：纯字符串操作改动 + 既有 helper 已被人工 UI 路
  径覆盖；jsdom 下 textarea selection mock 维护成本高。通过 vite build
  + 真实交互验证。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.22s)
