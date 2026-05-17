# PanelTasks 搜索框 `#` tag 自动补全 popover（iter #390）

## Background

PanelTasks 搜索框已支持 `#tag` 子串过滤（搜 `#工作` 命中所有
description 含该字符串的 task），但 owner 要敲对 tag 名 — 输错就漏
命中。detail.md 编辑器已有 `@` 自动补全 popover（输 `@` 弹 task
title 候选），本 iter 加 `#` 对偶：搜索框打 `#` 弹既有 tag 候选，
按频次排序，↑↓/Enter/Tab 接受。

## Changes

### `src/components/panel/PanelTasks.tsx`

#### 1. state（~line 4901，紧贴 atTrigger 系列）

```ts
const [tagDismissedAt, setTagDismissedAt] = useState<number | null>(null);
const [tagSelectedIdx, setTagSelectedIdx] = useState<number>(0);
const [searchCursorPos, setSearchCursorPos] = useState<number>(0);
```

完全镜像 atTrigger / atDismissedAt / atSelectedIdx 模板。

#### 2. tagTrigger memo

```ts
const tagTrigger = useMemo(() => {
  // 从 cursor 向回扫找 word-boundary `#`：遇 whitespace 即 abort；
  // 遇 `#` 时确认前一字符是 start / whitespace 才算 trigger（避免
  // `foo#bar` 误触）
}, [search, searchCursorPos, tagDismissedAt]);
```

与 atTrigger 同 word-boundary 扫描算法 — 防错误锚点（如 hex code
`#fff` 或 URL 片段）。

#### 3. tagSuggestions memo

```ts
const tagSuggestions = useMemo(() => {
  const counts = new Map<string, number>();
  for (const t of visibleTasks) {
    for (const tg of t.tags) counts.set(tg, (counts.get(tg) ?? 0) + 1);
  }
  // 频次降序 + tag 名 alphabetical tiebreak；query case-insensitive
  // substring 过滤；cap 8
}, [tagTrigger, visibleTasks]);
```

#### 4. acceptTagSuggestion + handleTagKeyDown

acceptTagSuggestion：`#query` 段（[hashPos, cursor)）替换为 `#<tag>`，
cursor 落 token 末尾 + rAF refocus input + setSelectionRange。

handleTagKeyDown：popover active 时拦 ↑↓/Enter/Tab/Esc — 与
handleAtKeyDown 同模板。

#### 5. 搜索框 UI 重构

- input 外包 `<div style="position: relative; flex: 1">` wrapper —
  popover absolute 锚到 input 底
- input 加 onSelect / onClick / onKeyUp 三 cursor tracker → setSearchCursorPos
- onChange 也同步 setSearchCursorPos（typed char 后 cursor 进位）
- onKeyDown 顶部接 `if (handleTagKeyDown(e)) return;`
- placeholder 加 "# 弹 tag 补全" 提示
- popover：粘 wrapper 底，max-height 220 + overflow-y auto；header
  显当前 query + 操作提示；每条 tag chip 显 `#name` + 频次 count；
  hover / ↑↓ 高亮（紫 tint，与 PanelMemory tag chips 同色族）

## Key design decisions

- **mirror @ popover pattern**：复用 atTrigger 的 word-boundary 算法、
  state 形态、accept/dismiss 协议。owner 学一次 @ 用法，#
  用法零认知开销。
- **频次排序而非 alphabetical**：常用 tag 浮顶 — owner 多数情况下
  想接的就是高频 tag。alphabetical 仅 tiebreak（同频次时稳定排
  序）。
- **`#tag` 替换而非补全后续字符**：与 detail.md `@` popover 同协
  议（`@query` → `「title」`）— 都用替换语义保 cursor 稳定 + 文本
  完整。
- **vs PanelMemory item 的 #tag chip click**：那个是已有 tag chip
  → 一键设为搜索；本 popover 是 owner 主动打 `#` 时弹候选。两路
  径正交（chip 是 visual entry，popover 是 typing entry）。
- **visibleTasks 而非 tasks 全集**：与"当前 filter 视图下出现的
  tag" 一致；owner 在 P7+ filter 下打 `#` 应弹 P7+ task 用的 tag
  不是全集。
- **searchCursorPos 多源 sync**：input cursor 移动可通过键盘 / 鼠
  标 / 输入 多路径触发；onChange + onSelect + onClick + onKeyUp 四
  入口覆盖几乎所有场景。React controlled input 没有原生 cursor
  pos state，必须手动跟踪。
- **不为单 fn 引 unit test runner**：行为是 IO + state ops；build
  pass + 手测足够（打 `#` / `#工` / `#工作 ` 三场景手测一遍）。

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.24s)
- 后端无改动 — 复用既有 task.tags 数组
