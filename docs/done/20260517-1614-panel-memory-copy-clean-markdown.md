# PanelMemory item「📑 复制纯 markdown」按钮（iter #312）

## Background

PanelMemory item action row 已有「📝 复制本条整段 markdown」按钮 — 但
输出格式是 `## title` H2 + 元数据 bullets（category / updated_at /
detail_path / 字数）+ `### Description` + `### detail.md` 多 section
结构。适合"share / 提 issue / 提 review"场景但对"外部转载 / 备份"（粘
到 Notion / Obsidian / 博客）来说太重 —— owner 想要的就是纯 markdown
正文，没有元数据噪音。

本迭代加「📑」按钮 — 同源 detail.md fetch，但输出极简 `# title` H1 +
description + detail.md 正文，无任何元数据 / section header。

## Changes

仅 `src/components/panel/PanelMemory.tsx`：

- 既有「📝」按钮之后插「📑」按钮：
  - 复用 `memory_read_detail_full` 拿 detail.md 全文（detailSizes > 0 时）
  - 拼装 `# {title}\n\n{description}\n\n{detail.md trimEnd}`
  - description / detail 任一空 → 该段省略（保持 markdown clean，无空段
    head）
  - clipboard.writeText + 3s setMessage toast 含字符数
  - tooltip 显式与「📝」对比："适合外部转载 / 备份 / 粘到 Notion /
    Obsidian / 博客 — 比「📝」更干净"

## Key design decisions

- **加新按钮而非改既有「📝」**：既有「📝」覆盖"完整含 meta 用于 issue
  tracking"场景被 owner 当下使用；改格式会破坏现有 UX。新「📑」覆盖"clean
  share" 场景，二者按用途分流让 owner 按场景选。
- **emoji 选 📑（页签 / 段落感）vs 既有 📝（笔记 / 写作感）**：与既有 📋
  detail.md 全文 / 📝 完整段 三者 emoji 各异避免视觉混淆。
- **H1 而非 H2**：`# title` 是顶级 header — owner 粘到 Notion / Obsidian
  时是一篇独立笔记的顶；既有「📝」用 H2 是因为它本身就是嵌入到更大 share
  context 里（issue / review）。语义匹配各自用途。
- **detail 段无 `---` 分隔 / 无 `### detail.md` 头**：纯 markdown 流是
  `# title` → description body → detail body 三段无标记拼接 — owner 粘
  出去的就是一篇可读 markdown 文章。任何额外分隔符都会让外部渲染时多
  一层无意义视觉噪音。
- **detail fetch 失败静默**：与「📝」"读取失败"占位文案不同 — 本按钮
  的场景是 clean export，失败时降级到 "title + description only" 比
  "title + description + 失败提示文字" 更符合 owner 意图（粘出去的别是
  错误信息）。
- **size === 0 时不 fetch**：避免无意义 IO；与「📋 detail.md 全文」按
  钮 gate 模式一致 — detailSizes 用 size==0 区分 "确认空" vs "未知"。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.20s)
