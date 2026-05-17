# detail.md 编辑器「📑 fold headings」toggle（iter #428）

## Background

长 detail.md（≥ 数千字）owner 看 preview 时想「鸟瞰目录结构」/
跳读各 H2 / H3 — 必须滚屏逐段读。本 iter 加 📑 fold headings
toggle — preview 模式下把 H2 / H3 段 body 折叠为占位 `> …（折
叠 N 字）`，仅 headings 露出，让 owner 拿到「目录视图」快速跳转。

textarea 自身无 native fold（HTML textarea 限制），本 toggle 仅
作用于 preview pane 的 parseMarkdown 入口数据；不动磁盘 / 不动
textarea state。

## Changes

### `src/components/panel/PanelTasks.tsx`

#### 1. `foldHeadings` state + localStorage 持久化

```ts
const [foldHeadings, setFoldHeadings] = useState<boolean>(() => {
  try {
    return localStorage.getItem("pet-detail-fold-headings") === "1";
  } catch { return false; }
});
const toggleFoldHeadings = useCallback(() => { ... }, []);
```

与 `previewRawMode` 同模板：localStorage key `pet-detail-fold-
headings`，owner 一次设置跨 task / 跨 session 保持。

#### 2. `foldHeadingsContent(md)` pure helper

```ts
const foldHeadingsContent = useCallback((md: string): string => {
  const lines = md.split("\n");
  const out: string[] = [];
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];
    const h = line.match(/^(#{2,3})\s+(.+)$/);
    if (!h) { out.push(line); i++; continue; }
    out.push(line);  // emit heading
    i++;
    // 收 body 直到下一个 H1 / H2 / H3
    const bodyStart = i;
    while (i < lines.length && !lines[i].match(/^#{1,3}\s+/)) i++;
    const bodyLines = lines.slice(bodyStart, i);
    const bodyChars = Array.from(bodyLines.join("\n")).length;
    const trimmedBody = bodyLines.join("").trim();
    if (trimmedBody.length > 0) {
      out.push(`> …（折叠 ${bodyChars} 字 · 关「📑」展开）`);
      out.push("");
    }
  }
  return out.join("\n");
}, []);
```

设计要点：
- **仅 fold H2 / H3**：H1 通常是全文标题不该折；H4+ 视为段内细分
  保完整阅读
- **section 收口 = 遇下一 H1/H2/H3**：H1 也算 boundary 让 H2 段
  正确结束在下一 H1 处
- **chars 用 unicode code points**：`Array.from().length` — 与既有
  字数 chip 统计语义一致，让 owner 看到的「N 字」与底部 status
  bar 同粒度
- **空 body 段不渲占位**：避免「## title \n > …（折叠 0 字）」
  空噪音；仅 trimmedBody.length > 0 时浮 placeholder
- **占位行用 `>` blockquote**：渲染为半色块状视觉 — 与正文段视
  觉区分，hint owner「这是折叠的占位非内容」

#### 3. Toggle button UI（紧贴 🆎 raw-mode 之后）

```tsx
{detailViewMode !== "edit" && !previewRawMode && (
  <button onClick={toggleFoldHeadings}
    style={{ ...blue-tint-when-active }}
    aria-pressed={foldHeadings}>
    📑 {foldHeadings ? "折叠" : "展开"}
  </button>
)}
```

- gate by `detailViewMode !== "edit"`：edit 纯 textarea 视图无 preview
  渲染入口，按钮无意义
- gate by `!previewRawMode`：raw 模式显原文不走 parseMarkdown，
  fold 无效；owner 切 raw 即「想看完整文本」语义自动 fold 不合理
- blue tint 与既有 🆎（amber）/ 复制 chip 区分；active 态加边框 +
  bold 直观

#### 4. parseMarkdown 入口两处接 fold

split 模式（line 13471）和 preview 模式（line 13628）的
`parseMarkdown(editingDetailContent, opts)` 改成
`parseMarkdown(foldHeadings ? foldHeadingsContent(...) : ..., opts)`。
parseMarkdown 不需修改 — 输入端拦截足够。

## Key design decisions

- **不递归 fold H4+**：H2 / H3 已足够「鸟瞰」；递归折叠让 outline
  过深 owner 反而难追溯。可后续 iter 加 fold level 配置
- **不在 edit textarea 内做 fold**：HTML textarea 不支持 fold；
  Monaco / CodeMirror 才有真 fold gutter — 引入完整编辑器是大改
  造，超本 iter 范围
- **不修改 parseMarkdown helper**：在 markdown 文本层 fold 而非 AST
  层切 — 与「复制本节」/「heading scrollIntoView anchor」等既有
  feature 解耦；headings 仍正常 render 含 anchor id
- **不为 fold 段加交互（click 展开单段）**：占位行只是 markdown
  blockquote — 不可点击展开单段。全展开 / 全折叠 toggle 已覆盖
  80% 场景；细颗粒度展开需另做 reduce/re-fold protocol
- **不为单 helper 引 unit test runner**：纯 string split + regex；
  build pass + 手测足够：（1）写带 H2/H3 + body 的 detail → 切到
  preview 看正常渲 →（2）click 📑 → 看 H2 body 替换为 `> …（折
  叠 N 字）` 占位 → headings 仍可见 →（3）再 click 切回展开
  →（4）切 🆎 raw 模式 → 📑 按钮消失（gate 生效）

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.42s)
- 后端无改动 — 纯前端 string 处理 + UI toggle
