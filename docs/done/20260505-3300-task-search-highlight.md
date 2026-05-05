# 任务面板搜索高亮命中 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> 任务面板搜索高亮命中：`PanelTasks` 搜索过滤已工作但 task title / body 里没把 query 子串高亮；复用既有 SearchResultRow / PanelSettings 的 `<mark>` 模式让命中位置一眼可见。

## 目标

`PanelTasks` 搜索过滤命中后只是把不匹配的行隐藏，命中行的 title / body 还是
原色，扫长结果时仍要二次定位 query 出现的位置。本轮在 row title 与 body 里
把 query 子串用 `<mark>` 浅黄高亮（与 PanelChat / PanelSettings 同色源 +
同 `mark` 模式，让"全 panel 搜索高亮"风格统一）。

## 非目标

- 不在 t.tags chip 里高亮命中 —— 现 tag 过滤是独立 chip-toggle 路径，与文本
  search 子串语义不同。
- 不在详情面板 raw_description / detail_md 里高亮 —— 这些段已在面板展开后单
  独可见，且本轮搜索是基于 task list 而非详情内容。
- 不写 README —— 任务面板可见性微调。

## 设计

### Pure 组件

PanelTasks 加 file-level `HighlightedText`（与 PanelSettings 的 `HighlightedText`
独立但行为相同 —— 不抽公共模块以保持每个 panel 的样式自治）：

```tsx
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
const HIGHLIGHT_MARK_STYLE: React.CSSProperties = {
  background: "#fef3c7",
  color: "#92400e",
  padding: "0 1px",
  borderRadius: 2,
};
```

### 应用

PanelTasks 行渲染 2 处替换：

```tsx
// 标题（既有：`{t.title}`）
<HighlightedText text={t.title} query={search} />

// body（既有：`{t.body && <div style={s.itemBody}>{t.body}</div>}`）
<HighlightedText text={t.body} query={search} />
```

不需要重排 layout —— `<HighlightedText>` 返回 inline `Fragment + <mark>`，与
原 `{string}` 在 flex / 文本流里完全等价。

### 测试

逻辑全 pure，与 PanelSettings 同形态。无 vitest，靠 tsc + 手测。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | `HighlightedText` 子组件 + `HIGHLIGHT_MARK_STYLE` |
| **M2** | row title / body JSX 替换 |
| **M3** | tsc + build + cleanup |

## 复用清单

- 既有 `search` 状态
- `<mark>` 配色与 PanelChat / PanelSettings 同源（黄底深棕字）

## 进度日志

- 2026-05-05 33:00 — 创建本文档；准备 M1。
- 2026-05-05 33:05 — 完成实现：`PanelTasks.tsx` 加 file-level `HighlightedText` + `HIGHLIGHT_MARK_STYLE`（与 PanelChat / PanelSettings 同色源），row title 与 body 渲染替换为 `<HighlightedText>`。`pnpm tsc --noEmit` 干净；`pnpm build` 497 modules 全过。TODO 移除条目；本文件移入 `docs/done/`。
  - **README 不更新** —— 任务面板可见性微调。
  - **未做手动 dev 验证**：当前会话不便启动 Tauri 桌面 app；HighlightedText pure 与 PanelSettings 同形态。
