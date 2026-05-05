# PanelMemory 搜索结果高亮匹配子串（Iter R88）

> 对应需求（来自 docs/TODO.md）：
> PanelMemory 搜索结果高亮匹配子串：现在搜索后只过滤列表，结果里看不到关键字在 title / description 中的位置；用 PanelChat / PanelSettings 同款 HighlightedText 加上黄底高亮。

## 目标

PanelMemory 搜索框点回车 / "搜索"按钮后，结果以列表形式显示 `r.title` /
`r.description`，纯文本无标记。当 description 较长时，用户得肉眼扫匹配位
置 —— 与 PanelChat 跨会话搜索 / PanelSettings 设置搜索的"黄底高亮"体验
不一致。

加上 HighlightedText 包装：黄底深棕字 (`#fef3c7` / `#92400e`) 标出关键字
在 title / description 中第一次出现的位置，与 PanelTasks / PanelSettings
同款。

## 非目标

- 不抽 HighlightedText 到共享 module —— 当前已在 PanelTasks / PanelSettings
  各 inline 一份（12 行 + 5 行常量）。第三处 inline 仍处于"未达抽象阈值"
  的灰区；按 CLAUDE.md "Three similar lines is better than premature
  abstraction" 留待后续如有第 4 处再统一提取
- 不做多次出现的全部高亮 —— 与既有两份 HighlightedText 一致：仅命中第一处。
  扩展到 `[start, end][]` 多段重渲染会动到字符串切片接口；本轮不展开
- 不 highlight `r.category` chip —— 那是 enum-like 短串，已经一眼可见

## 设计

### 复制 helper（与 PanelTasks 完全一致）

放在 PanelMemory 函数体内最末，紧邻 export default 之前：

```ts
const HIGHLIGHT_MARK_STYLE: React.CSSProperties = {
  background: "#fef3c7",
  color: "#92400e",
  padding: "0 1px",
  borderRadius: 2,
};

function HighlightedText({ text, query }: { text: string; query: string }) {
  const q = query.trim();
  if (q.length === 0) return <>{text}</>;
  const idx = text.toLowerCase().indexOf(q.toLowerCase());
  if (idx < 0) return <>{text}</>;
  return (
    <>
      {text.slice(0, idx)}
      <mark style={HIGHLIGHT_MARK_STYLE}>{text.slice(idx, idx + q.length)}</mark>
      {text.slice(idx + q.length)}
    </>
  );
}
```

### 渲染替换

```diff
- <div style={s.itemTitle}>{r.title}</div>
+ <div style={s.itemTitle}><HighlightedText text={r.title} query={searchKeyword} /></div>
...
- <div style={s.itemDesc}>{r.description}</div>
+ <div style={s.itemDesc}><HighlightedText text={r.description} query={searchKeyword} /></div>
```

### 关于 query 时机选择

用 `searchKeyword`（input 当前值）而不引入额外 `lastSearchedKeyword` state：
- 用户点搜索后，结果与 keyword 一致 → 高亮准确
- 用户继续输入：input 变化 → 高亮跟随，但 results 是 stale。出现"highlight
  在 r.text 里找不到"时 HighlightedText 自然降级为原文（idx < 0 路径）
- 多余复杂度（额外 state + 在 handleSearch 里同步）换不到明显收益

这个权衡与 PanelChat SearchResultRow（直接用 hit-time match_start）路径
不同 —— PanelChat 是后端返了精确 match offset；PanelMemory 没该字段，
前端做近似匹配即可。

### 测试

无单测；手测：
- 搜 "claude" → results 列表里 "claude" 子串变黄底深棕
- 搜 "Claude"（大小写）→ 仍命中（toLowerCase 双向）
- 搜 "不存在" → 返空列表 + "未找到匹配项" placeholder
- 搜 "abc" 后继续输入到 "abcdef" → 高亮跟随当前 input（results 是 stale，
  match 不到时自动降级为原文）

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | 加 helper + 替换 2 处渲染 |
| **M2** | tsc + build |

## 复用清单

- 既有 `s.itemTitle` / `s.itemDesc` 样式不动
- HighlightedText 接口与 Tasks / Settings 一致（text + query props）

## 进度日志

- 2026-05-08 16:00 — 创建本文档；准备 M1。
- 2026-05-08 16:08 — M1 完成。PanelMemory 文件末加 `HIGHLIGHT_MARK_STYLE` const + `HighlightedText` helper（与 PanelTasks / PanelSettings 完全一致）；搜索结果 r.title / r.description 两处用 HighlightedText 包装，query 传 searchKeyword。
- 2026-05-08 16:11 — M2 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (499 modules, 961ms)。归档至 done。
