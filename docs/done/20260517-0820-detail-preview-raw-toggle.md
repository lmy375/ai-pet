# detail.md preview「🆎 切纯文本」toggle（iter #269）

## Background

detail.md 编辑器已有 ✏️ edit / 🔀 split / 👁 preview 三态切换。在 split /
preview 模式下，右侧 / 主面板用 `parseMarkdown` 渲染 markdown — 链接成
chip / 标题成 styled heading / `「ref」` 成 hover-able 引用。

但 owner 调 markdown 语法时（"这段加粗为啥没生效？"）想看 raw 文本对比；
或想一键 ⌘A 全选复制原 markdown 时，渲染后的视图会把 link / ref token 等
转成 component，selectAll + copy 拿到的不是纯文本。

本迭代加 🆎 toggle：on 时把 preview pane 渲成 `<pre>` 原文（保留所有空白
+ 换行 + 等宽字体），off 时回 parseMarkdown 渲染。状态持久化 localStorage
跨会话保留偏好。

## Changes

仅 `src/components/panel/PanelTasks.tsx`：

- **state**：
  - `previewRawMode: boolean` + `pet-detail-preview-raw` localStorage 持久化
  - `togglePreviewRawMode` useCallback：取反 + 写 localStorage

- **toggle 按钮**：在 ✏️/🔀/👁 三态切换之后插「🆎 原文 / 渲染」按钮：
  - 仅 `detailViewMode !== "edit"` 时显（edit 模式无 preview 段无意义）
  - on：amber tint（`var(--pet-tint-amber-bg/fg)`）— 与既有"⚠ 警示态"
    色系连贯，让 owner 知道"当前是非默认渲染模式"
  - off：muted 灰底
  - aria-pressed 反映状态

- **两处 parseMarkdown 调用点改造**：split 模式右 pane（line 9999）+ preview
  模式（line 10034）：
  - `editingDetailContent.trim() === "" ? <空提示> : previewRawMode ? <pre raw> : parseMarkdown(...)`
  - `<pre>` 用 `whiteSpace: pre-wrap` / `wordBreak: break-word` / `'SF Mono'`
    等宽字体 / fontSize 12 / lineHeight 1.65 / margin 0，与既有 textarea
    fontFamily / size 一致让"原文 vs 编辑态"视觉对齐。

## Key design decisions

- **同时影响 split + preview 两模式**：toggle 语义是"preview pane 渲什么"，
  在 split（preview 是右半）和 preview-only（preview 是整面）都适用同一
  渲染开关。
- **不影响 edit 模式**：edit 没 preview pane，按钮不显避免噪音 + 误触。
- **`<pre>` 加 `wordBreak: break-word`**：长 URL / 中英文混排不会撑爆面板
  宽度（preview pane 在 split 模式下宽度约一半 panel）；保留 `whiteSpace:
  pre-wrap` 让 `\n` 仍换行且 leading 空白保留。
- **localStorage 持久化**：与 detailViewMode / showDetailGutter 同模板。
  owner 习惯 raw mode 调 markdown 后跨会话保留偏好，下次启动不必再开。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.25s)
