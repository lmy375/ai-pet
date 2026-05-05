# 任务详情笔记 markdown 预览开关 — 开发计划

> 对应需求（来自 docs/TODO.md）：
> 任务详情笔记 markdown 预览开关：detail.md 编辑器现只显原始文本；加 toggle "渲染 / 源码"，渲染态用 inline markdown parser（已有的 `parseMarkdown`）让 checkbox / 列表 / 粗体看见效果。

## 目标

PanelTasks 任务详情的 detail.md 当前以 mono 等宽字体 + `pre-wrap` 显原始
文本，看不到 markdown 渲染效果（`**bold**` / `- 列表项` / inline code 都
是字面文本）。本轮在「进度笔记」段头部加个 "渲染 / 源码" toggle，渲染态
用现成的 `parseMarkdown`（src/utils/inlineMarkdown.tsx，已用于桌面气泡）把
basic markdown 视觉化。

## 非目标

- 不扩展 parseMarkdown 解析器 —— `- [ ]` / `- [x]` 当前会渲染成普通 bullet
  + 字面文本，对用户而言意思仍清晰；为了任务面板独自加 checkbox 解析会让
  桌面气泡 / settings 等其它复用方接 surprise，得不偿失。
- 不 per-task 持久化 —— 全局 toggle 已足够；per-task 需要 Map<title, mode>，
  state 复杂度上升而价值低。
- 不 localStorage 持久 —— 与既有 PanelTasks / PanelDebug 切换型 state 一致
  （临时阅读姿态，重启走默认）。
- 不影响编辑模式 —— 编辑 textarea 仍是源码（writing 永远要 raw）。toggle
  只切浏览态。

## 设计

### state

`detailMdRenderMode: "rendered" | "source"` default `"rendered"` — 渲染默
认更"友好"，用户偶尔需要查 raw 时再切。

### UI

「进度笔记 (detail.md)」标签行后面、`复制` 按钮**之前**插一个小 toggle：
- 文案 `🅼 源码` / `🅼 渲染`（点击切换；当前模式相反的目标态做按钮文字
  → 与"点击会切到 X"直觉一致）
- 视觉与既有 detail-section 小按钮一致（10px 字、灰边、白底）

仅当 `editingDetailTitle !== t.title && detail.detail_md.trim()` 时渲染
toggle —— 与现有「复制」按钮的可见条件一致。

### 渲染分支

```tsx
{editingDetailTitle === t.title ? (
  /* 编辑 textarea，保持原样 */
) : detail.detail_md.trim() ? (
  detailMdRenderMode === "rendered" ? (
    <div style={s.detailMdBox}>{parseMarkdown(detail.detail_md)}</div>
  ) : (
    <div style={s.detailMdBox}>{detail.detail_md}</div>
  )
) : (
  <div style={s.detailHint}>宠物还没写进度笔记</div>
)}
```

`s.detailMdBox` 已有 mono 字体；渲染模式下 mono 让 inline code 仍清晰。
parseMarkdown 输出的 `<div>` 列表自带换行，与 `pre-wrap` 不冲突。

## 测试

PanelTasks 是 IO 重容器；前端无 vitest，靠 tsc + 手测。parseMarkdown 已
在桌面气泡 / SearchResultRow 等多处使用，有自己的覆盖。

## 阶段划分

| 阶段 | 范围 |
| --- | --- |
| **M1** | detailMdRenderMode state + toggle 按钮 |
| **M2** | 渲染分支接 parseMarkdown |
| **M3** | tsc + build + cleanup |

## 复用清单

- 既有 `parseMarkdown` 函数
- 既有 detail-section 小按钮视觉
- 既有 `s.detailMdBox` 容器样式

## 进度日志

- 2026-05-07 07:00 — 创建本文档；准备 M1。
- 2026-05-07 07:10 — M1 完成。`parseMarkdown` 从 utils 导入；`detailMdRenderMode` state 默认 "rendered"；section header 复制按钮前加 toggle（文字反映"点击会切到 X"），仅 detail.md 非空 + 浏览态时渲染。
- 2026-05-07 07:15 — M2 完成。浏览态渲染分支按 mode 切换 parseMarkdown(text) / 字面 text；编辑模式不动。
- 2026-05-07 07:20 — M3 完成。`pnpm tsc --noEmit` 0 错误；`pnpm build` 通过 (498 modules, 952ms)。归档至 done。
