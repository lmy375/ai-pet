# PanelMemory ⌘K 跨 cat memory quick-find palette（iter #245）

## Background

PanelMemory 已挂 ⌘F / Ctrl+F → 聚焦顶部搜索框（与 mac Finder / 浏览器直觉对齐）。
但顶部搜索框是「敲完 Enter 才查后端」的传统流程，对「我记得某 item 大概叫什
么、想立刻跳到它的位置 / 编辑它」的场景体验差：要敲 + 等结果 + 在 N 条 result
里 click + 还得知道在哪个 cat。

owner 已在 PanelTasks（iter #240）习惯了 ⌘K 即时 fuzzy palette → ↑↓ → Enter 直
接跳的 muscle memory，本迭代把同套模板移到 PanelMemory，跨所有 cat 一键找到目
标 item 并跳转。

## Changes

仅 `src/components/panel/PanelMemory.tsx`：

- **state**（在 ⌘F 监听后插入）：
  - `memPaletteOpen / memPaletteQuery / memPaletteSelectedIdx / memPaletteInputRef`
  - `memFlashKey`（Enter 跳到 item 后 1.6s 闪烁标记）

- **⌘K 全局监听**：与 ⌘F 同模式，`window.addEventListener("keydown")`，
  metaKey/ctrlKey + 无 shift/alt + `k` → setMemPaletteOpen(true)。

- **`allMemoryItems` useMemo**：把 index.categories 按 CATEGORY_ORDER 顺序
  flatten 成 `{ catKey, catLabel, title, description }[]`，供 fuzzy 过滤。

- **`jumpToMemoryItem(catKey, title)` callback**：palette Enter 时调
  1. `setMemPaletteOpen(false)`
  2. 清 `searchKeyword` + `searchResults`（不然 searchResults gate 会把整 cat
     树藏掉，scrollIntoView 找不到 DOM）
  3. 把 catKey 加入 `expandedCategories` 并持久化到 localStorage
     `pet-memory-expanded-cats`（与 iter #5001 持久化路径一致）
  4. 50ms 后 `document.querySelector("[data-mem-key=...]")` → `scrollIntoView`
     + setMemFlashKey 触发 1.6s 黄边闪烁

- **item div 加 `data-mem-key={`${catKey}::${item.title}`}`**：scrollIntoView
  查询锚点。同时把 `memFlashKey` 命中态映射到 outline (黄底色) — 让 owner 视
  觉锁定 scroll 终点。

- **palette overlay**（在最终 `</div>` 前插入）：与 PanelTasks ⌘K palette 同
  样的 fixed inset:0 + backdrop + 480 / 520px 卡片 + input + 30 条候选 + ↑↓
  / Enter / Esc / mouse hover 同步 selectedIdx。右侧 chip 显类目（优先用
  categoryLabels 自定义名，否则 cat.label）。

## Key design decisions

- **search 与 title 双匹配**：`title.includes(q) || description.includes(q)`，
  让 owner 不记得 title 但记得 description 关键词时也能命中。
- **清搜索 + 展开 cat 必须**：不清搜索时 PanelMemory 走的是 search-result
  render path，category 树消失 → scrollIntoView 找不到 DOM。展开 cat 是因为
  collapsed 状态 shownItems 是切片，目标可能不在 DOM 内。
- **50ms setTimeout**：等 React 重渲染（清 search + 展开 cat 改了 state），
  下一帧 DOM 才有目标 row。比 requestAnimationFrame 更稳，主流程不依赖动画。
- **黄边 outline 而不是 background flash**：item div 已有 hover preview tooltip
  叠在 `position: relative` 上，改 background 会污染原配色 / hover 态。outline
  是 box-model 外的视觉层，不动布局。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.17s)

## Notes

PanelTasks ⌘K（detail.md 编辑器内）与 PanelMemory ⌘K（永远生效）不冲突 —
两个 panel 被 PanelApp 按 activeTab 互斥挂载，同时只有一个 ⌘K listener 注册。
