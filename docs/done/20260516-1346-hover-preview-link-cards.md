# 任务行 hover preview 段也走 LinkCard

## 背景

TODO 上 auto-proposed 一条："任务行 hover preview 段的 detail.md 文本也走 LinkCard：当前仅展开任务的 detail 段渲染 chip，hover preview 文本还是纯文本。"

任务行 hover preview 显 detail.md 前 600 字 + 元数据 chips + 最近 3 条 history。其中 detail 文本段当前用 `<div style={{ whiteSpace: "pre-wrap" }}>{detailSnippet}</div>` 渲染 —— bare URL 显成纯文本，owner 想从 hover 看到"这条任务引用了哪个外部资源"还得展开任务才能看到 emoji chip。

把 hover preview 文本段也走 `renderDetailTextWithLinkCards`，但用更轻量的 `"raw"` 模式（不跑 parseMarkdown）—— 保留既有"raw markdown 字面"视觉 + 加上 URL chip 化，让常用引用一眼可见。

## 改动

### `src/components/panel/PanelTasks.tsx`

#### `renderDetailTextWithLinkCards` 加 textMode 参数

```ts
function renderDetailTextWithLinkCards(
  text: string,
  keyPrefix: string,
  textMode: "markdown" | "raw" = "markdown",
): ReactNode[] {
  const renderChunk = (s: string, key: string): ReactNode =>
    textMode === "markdown" ? (
      <Fragment key={key}>{parseMarkdown(s)}</Fragment>
    ) : (
      <Fragment key={key}>{s}</Fragment>
    );
  // ... rest unchanged ...
}
```

默认 `"markdown"` 让既有调用（parseDetailMdWithImages 展开 detail）行为完全不变。新增 `"raw"` 模式给 hover preview 用。

#### hover preview 渲染段

原：

```tsx
<div style={{ whiteSpace: "pre-wrap" }}>
  {detailSnippet}
</div>
```

改：

```tsx
<div style={{ whiteSpace: "pre-wrap" }}>
  {renderDetailTextWithLinkCards(detailSnippet, `hover-${t.title}`, "raw")}
</div>
```

key prefix 用 `hover-${title}` 避免与展开详情段（`txt-{idx}` / `txt-tail`）的 key 冲突 —— 同一文档 hover preview 与 detail 展开可同时存在（任务被 hover 又被展开），各自的 React 子树独立。

## 关键设计

- **`textMode: "raw"` for hover preview**：hover 是 high-frequency / low-latency 场景 —— owner 鼠标在行间扫时 preview 闪现闪没。如果每次 hover 都跑 parseMarkdown（split lines / regex parse fenced code / 渲 lists / tables / blockquote 等），延迟会肉眼可见。`raw` 模式只做 URL split，非 URL 段直接放原文，preserves the `pre-wrap` 视觉。
- **`textMode: "markdown"` 默认**：既有 parseDetailMdWithImages 调用方不需要改 —— TS 默认参数让 backward compat 0 代价。
- **key prefix 隔离**：hover preview 与展开 detail 段都用 `renderDetailTextWithLinkCards` 但 prefix 不同。React 在 reconciliation 时不会把两个子树的 key 混淆 —— 即便偶发 hover-then-expand 同一行，渲染稳定。
- **`raw` 模式下 0 URL 时返 `[text]`**：与 `markdown` 模式返 `[parseMarkdown(text)]` 对偶 —— 都返单条 ReactNode 避免 splice 空 array。
- **不动 detail.md 展开段**：那里继续 markdown 模式，让正式渲染体验完整（bold / code / lists 等）。hover 只是 quick peek，简化是合理 trade-off。
- **不引入 memo 缓存 LinkCard**：LinkCard 内部已是纯函数 + 单层 a 标签；React 重渲成本接近 0。hover 闪现的成本主要在 parseMarkdown，raw 模式已绕过。

## 不做

- **不在 hover preview 渲 markdown 富格式**：性能 + 视觉一致性 trade-off 选 raw。owner 想看 rendered markdown 展开任务即可。
- **不写测试**：纯字符串 split + ReactNode 构造，逻辑分支扩 1 个 mode；既有 detail.md 路径无单测。视觉验证（hover 含 URL 的 detail.md → emoji chip 浮现）足够。
- **不动 inlineMarkdown / panelChatBits 共享 helpers**：当前 LinkCard 与 textMode 都是 PanelTasks 局部 helper；要复用到其它文件再抽 shared utils。
- **不让 expand 段也支持 raw 模式**：那里就该 markdown 富渲染，简化没好处。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.19s
- 改动 ~25 行（textMode 参数 + renderChunk helper + hover preview callsite 替换 + 注释）；既有 parseDetailMdWithImages / LinkCard / pickLinkEmojiAndLabel 路径完全不动。

## TODO 状态

empty —— 6 条 auto-proposed 全部完成（其中 1 条 stale 已移除）。下次启动 TODO 流程进入 auto-propose 分支。

## 后续

- "raw" 模式扩到其它"快显"场景（如归档列表 / search result snippet 等），让 LinkCard chip 在所有 detail-summary 视图都呈现 emoji。
- hover preview 加 "📎 含 N 个链接" 顶部 chip 让 owner 还没读 detail 就知道有多少外部引用 —— 比 LinkCard chip 嵌在文中更显眼。
- markdown 模式按需 memoize：若用户回到 hover-and-leave 频繁场景且 preview 还包含展开内容，可考虑 `useMemo(parseMarkdown(text), [text])`。当前 hover 是单 task 一次性触发，无 memo 必要。
