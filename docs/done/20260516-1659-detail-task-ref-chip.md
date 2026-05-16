# detail.md `[task: 标题]` 语法 → 任务 ref chip

## 背景

TODO 上 auto-proposed 一条："detail.md preview `[task: 标题]` 语法识别为任务 ref chip（与 chat「标题」ref 同 hover preview）—— 让 owner 在 detail.md 也能 link 到其它任务。"

chat 已支持 `「title」` ref token（PanelChat 内 `@` mention picker 输出 + 双击跳转）。detail.md 跨任务引用场景同样常见 —— owner 在某条 detail.md 写"接 task: 整理 Downloads 之后做"，想点 chip 直接跳过去。

补一个 `[task: 标题]` 语法专给 detail.md，可识别 chip 化 + click 跳焦点。

## 改动

### `src/components/panel/PanelTasks.tsx`

#### 新 `statusEmojiForTask` helper

```ts
function statusEmojiForTask(status: string | undefined): string {
  switch (status) {
    case "done": return "✅ ";
    case "error": return "❌ ";
    case "cancelled": return "🚫 ";
    default: return "📋 "; // pending / unknown
  }
}
```

#### 新 `TaskRefChip` 组件

```tsx
function TaskRefChip({ title, taskInfo, onClick }) {
  const found = taskInfo !== null && taskInfo !== undefined;
  const emoji = found ? statusEmojiForTask(taskInfo!.status) : "❓ ";
  const pinPrefix = found && taskInfo!.pinned ? "📌 " : "";
  return (
    <button
      onClick={(e) => { e.preventDefault(); e.stopPropagation(); onClick?.(title); }}
      title={found ? `跳到任务「${title}」（status: ${status}）` : `引用了任务「${title}」，但未找到`}
      style={{
        display: "inline-flex",
        background: found ? "var(--pet-tint-blue-bg)" : "muted gray",
        border: found ? "1px solid blue tint" : "1px dashed muted",
        ...
      }}
    >
      {pinPrefix}{emoji}{title}{!found && <span>(未找到)</span>}
    </button>
  );
}
```

- found = blue tint chip + status emoji + (optional) 📌 pin prefix
- not-found = muted dashed border + "(未找到)" 后缀 + dimmed

#### `renderDetailTextWithLinkCards` 扩 task ref alternation

签名加两个可选参数：

```ts
function renderDetailTextWithLinkCards(
  text, keyPrefix, textMode = "markdown",
  taskLookup?: (title) => { status; pinned? } | null,
  onTaskClick?: (title) => void,
): ReactNode[] {
  const COMBINED_RE = taskLookup
    ? /(?<!\]\()https?:\/\/[^\s)\]<>"']+|\[task:\s+([^\]]+?)\]/g
    : /(?<!\]\()https?:\/\/[^\s)\]<>"']+/g;
  // ... 循环中 group 1 命中 → TaskRefChip；否则 LinkCard ...
}
```

alternation regex：URL OR task ref。group 1（task title）命中时本 match 是 task ref；否则 URL。

#### `parseDetailMdWithImages` 透传两个参数

```ts
function parseDetailMdWithImages(md, onOpenImage, taskLookup?, onTaskClick?) {
  // ... 内部 renderDetailTextWithLinkCards 调用都传 taskLookup, onTaskClick ...
}
```

#### PanelTasks 组件层接入

```ts
const taskLookupForRefs = useCallback(
  (title: string) => {
    const found = tasks.find((t) => t.title === title);
    if (!found) return null;
    return { status: found.status, pinned: !!found.pinned };
  },
  [tasks],
);

const handleTaskRefClick = useCallback((title: string) => {
  // 复用既有 pendingTitleFocus 路径 —— 清 filter / 显 finished / 写 title
  setSearch("");
  setSelectedTags(new Set());
  setDueFilter("all");
  setPriorityFilter(new Set());
  setOriginFilter(new Set());
  setPinnedFilter(false);
  setShowFinished(true);
  setPendingTitleFocus(title);
}, []);
```

#### 两个 callsite 传入

1. **detail 展开 read-only 区**：`parseDetailMdWithImages(detail.detail_md, setLightbox, taskLookupForRefs, handleTaskRefClick)`
2. **hover preview 段**：`renderDetailTextWithLinkCards(detailSnippet, hover-key, "raw", taskLookupForRefs, handleTaskRefClick)`

## 关键设计

- **`[task:\s+(.+?)\]` 严格要求冒号后空格**：与 task description marker `[task pri=...]` 视觉错开；owner 写 `[task: 标题]` 才匹配，不抢 `[task pri=3]` 类形态（虽然后者只出现在 description 不在 detail.md body）。
- **alternation regex 单次扫描**：把 URL_RE 和 TASK_REF_RE 合并 alternation 一次走完 —— 避免两次 scan + sort match positions。group 1 捕获 task title 子段，命中即知是 task ref。
- **taskLookup 不传时关掉整个 task ref 识别**：让既有 callsite (raw text mode / 没 task context 的场景) 0 行为变化 —— `[task: ...]` 字符串照样 raw 出现。
- **`found vs not-found` 双视觉态**：found（blue tint + 强 chip border）= cross-link 可用；not-found（muted dashed + "(未找到)"）= owner 引用了已删 / typo 任务，仍渲 chip 让 owner 看到此处有引用但需修。
- **复用 pendingTitleFocus pipeline**：既有 "完成小卡 click title 跳行" 已经走这条路径（清 filter + 显 finished + 写 title → 下一帧 effect 找 idx + scrollIntoView + focus）。task ref click 接入同 pipeline UX 一致。
- **不修改 parseMarkdown**：preview / split 模式仍走 parseMarkdown 不识别 `[task:]`。这是有意的 scope —— 编辑期间 owner 看到自己输入的字面量；保存后 read-only 展开渲 chip。修 parseMarkdown 会影响 chat / mini chat / memory 等 5+ callsite，blast radius 大。
- **pinPrefix 在 emoji 前**：📌 是 owner 显式钉住的强信号，应优先于 status emoji 可见。

## 不做

- **不接 preview 模式 chip 渲染**：见"不修改 parseMarkdown"理由。owner 写完保存后 read-only 视图就有 chip。
- **不写测试**：纯 regex + ReactNode 构造；既有 LinkCard 同模式无单测。视觉验证（写 `[task: 整理 Downloads]` 进 detail.md → 保存 → 展开 → 看到 blue chip → 点击跳行）足够。
- **不接桌面 ChatMini / PanelChat textarea**：chat 已有自己的 `「title」` ref 语法 + @ picker；不需双轨。本 iter 专注 detail.md 场景。
- **不允许 onClick 无 lookup 时仍触发**：lookup 返 null 时 chip click 仍调 onClick(title)；让 setPendingTitleFocus 自然 no-op（找不到 idx）—— owner 能看到"chip 点了没反应"自我 debug。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.20s
- 改动 ~170 行（statusEmojiForTask helper 12 + TaskRefChip 组件 70 + renderDetailTextWithLinkCards 扩 alternation 30 + parseDetailMdWithImages 透传 8 + 组件层 taskLookupForRefs / handleTaskRefClick 30 + 两 callsite 接入 8 + 注释）；既有 LinkCard / URL chip 路径 / parseMarkdown 全部 callsite / 编辑器 preview/split parseMarkdown 调用完全不动。

## TODO 状态

6 条 auto-proposed 已完成 3 条，余 3 条留池：
- PanelTasks detail 编辑器加「↑ 上 / ↓ 下一条」导航箭头
- 桌面 pet hover 3s 浮 ambient 三段统计微卡片
- PanelMemory 类目 7 天 churn sparkline

## 后续

- `[task: 标题]` 也接入 hover preview 不仅显 chip 还浮 "目标 task 的 mini preview"（含目标 task 的 status / due / 最近 1 条 history）—— 与既有 task hover preview 同源。
- 修 parseMarkdown 支持 chip 让编辑期间 preview 也渲（与编辑写作流体感一致）—— 但要小心 chat / memory 等 callsite 不受影响。
- 加 ` 「title」` 同款支持（与 chat 同 ref 语法对偶，让 owner 选用 IDE-style `[task: ]` 或 IM-style 「」 都行）。
- LLM 在 `butler_task_edit` description 工具描述中显示"detail.md 可写 `[task: 别人的标题]` 引用其它 task"教学，让宠物也学会跨链接。
