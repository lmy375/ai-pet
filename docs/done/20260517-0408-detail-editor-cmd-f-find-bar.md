# detail.md 编辑器内 ⌘F 全文搜浮 bar（iter #246）

## Background

detail.md 是 task 的进度笔记，长 task 累积下来很容易上千字（owner 的实际使用
里有 ≥ 5000 字的 detail）。在 textarea 里靠 ⌘↓ / 滚轮找一个关键词位置非常痛
苦。PanelTasks 顶部已有 task 列表搜索框（⌘F / ⌘K / `/`），但它搜的是 task 标题
与 raw_description，不进 detail.md 文本。

本迭代在 detail 编辑器内加 ⌘F → 浮 search bar（Chrome / VS Code 找一致），让
owner 在长 detail 里直接定位。

## Changes

仅 `src/components/panel/PanelTasks.tsx`：

- **state**（在 `detailEditorRef` 后插入一组）：
  - `detailSearchOpen / detailSearchQuery / detailSearchActiveIdx`
  - `detailSearchInputRef`
  - `detailSearchMatches` useMemo：扫 `editingDetailContent` 拿所有
    case-insensitive substring `{ start, end }`

- **切 task 重置**：useEffect 监听 `editingDetailTitle`：null 时关闭 + 清
  query；非 null 但切换时仅重置 idx（保留 query 让 owner 用同关键词跨 task）。

- **⌘F 劫持 effect**：`window.addEventListener("keydown", onKey, { capture: true })`。
  仅当 `activeElement === detailEditorRef.current || activeElement ===
  detailSearchInputRef.current` 时拦截 → `preventDefault +
  stopImmediatePropagation` → 开 bar + 聚焦自家 input。其他焦点位置下
  ⌘F 仍走 `useTaskKeyboardNav` 默认（聚焦顶部 task 搜索框）。

- **activeIdx 同步**：useEffect 监听 `[detailSearchActiveIdx, detailSearchMatches,
  detailSearchOpen]` → `textarea.focus() + setSelectionRange(start, end)` 让
  textarea 自动滚到 match → `requestAnimationFrame` 后 refocus 搜索 input
  让连按 Enter / ↑↓ 不丢焦点。

- **`cycleDetailSearchMatch(dir)` callback**：next/prev wrap，matches 空时
  noop。

- **UI bar**（在 `editingDetailTitle === t.title ? (` 的 wrapper 顶部，draft
  banner 之前）：
  - 🔍 icon + input + N/M 计数 chip（0/0 时红字提示无命中）+ ↑↓ 翻 +
    ✕ 关
  - input.onKeyDown：Enter → next（⇧Enter → prev）/ ArrowDown / ArrowUp /
    Esc（关 + refocus textarea）
  - ↑↓ ✕ button 同步 cycle / close

## Key design decisions

- **capture: true + stopImmediatePropagation**：`useTaskKeyboardNav` hook 的
  ⌘F listener 永远 focus 顶部 search input。capture phase 比 bubble phase 先
  跑，stopImmediatePropagation 拦下后续 window-level listener，保证 detail
  编辑器内 ⌘F 不会"双向触发"（既开 detail bar 又抢走焦点到顶部）。
- **仅当 focus 在 detail textarea / 自家 input 时劫持**：让其他焦点位置（顶
  部搜索框、列表）⌘F 继续走原 nav hook 路径，保持心智模型一致。
- **setSelectionRange 自动滚 textarea**：WebKit / Tauri webview 内 textarea
  在 focus + setSelectionRange 后会把 selection scroll 进可视区。无需自己算
  行高 / scrollTop。
- **refocus input via rAF**：连按 Enter 时若 textarea 抢走焦点，下一次 Enter
  就走 textarea 的 onKeyDown（不是 input 的）。rAF 等浏览器把 textarea 滚动
  画好后立即把焦点夺回，让连按 Enter 始终走 input handler → cycle 顺畅。
- **切 task 保留 query**：owner 在某 task 找完 keyword 切到另一 task，常常
  想用同关键词在新 task 内找；保留 query 减少重复输入。仅清 idx 防止越界。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.17s)

## Notes

textarea 选区高亮是 native 行为 — 关 bar 后选区仍保留（owner 可继续从该位置
往后编辑）。与 Chrome / VS Code 找完关闭后光标停在最后一个 match 的体验对齐。
