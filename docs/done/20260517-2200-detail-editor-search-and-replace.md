# detail.md 编辑器 ⌘⇧F 全文 search & replace popover（iter #402）

## Background

detail.md 编辑器既有 ⌘F find popover（iter 历史已实现）— 命中
计数 + ↑↓ 翻 match + Esc 关。但只能查不能改：owner 在长 detail.md
（几千字）里想批量改一个 typo / 调整术语统一称呼时仍需逐处手敲，
没有 VSCode 风的「替换为…」+ 「全部替换」入口。

本 iter 扩 ⌘⇧F 加 replace 半边 — 与 VSCode `⌘⇧F` 习惯对齐
（VSCode 用 ⌘F find / ⌘H replace 双绑；web 端 ⌘⇧F 单键覆盖更
直觉）。

## Changes

### `src/components/panel/PanelTasks.tsx`

#### 1. State 新增

```ts
const [detailReplaceMode, setDetailReplaceMode] = useState(false);
const [detailReplaceText, setDetailReplaceText] = useState("");
const detailReplaceInputRef = useRef<HTMLInputElement>(null);
```

切到另一 task（editingDetailTitle 变 / 变 null）时 reset 防 stale
state；与既有 detailSearchQuery 切 task 行为对齐。

#### 2. ⌘⇧F 全局快捷键 handler（capture phase）

与既有 ⌘F handler 同模板但带 shift 修饰：

```ts
useEffect(() => {
  if (editingDetailTitle === null) return;
  const onKey = (e: KeyboardEvent) => {
    if (!(e.metaKey || e.ctrlKey)) return;
    if (!e.shiftKey || e.altKey) return;
    if (e.key.toLowerCase() !== "f") return;
    const ae = document.activeElement;
    if (ae !== textareaRef && ae !== searchInputRef && ae !== replaceInputRef) return;
    e.preventDefault();
    e.stopImmediatePropagation();
    setDetailSearchOpen(true);
    setDetailReplaceMode(true);
    setDetailSearchActiveIdx(0);
    setTimeout(() => {
      if (!detailSearchQuery) detailSearchInputRef.current?.focus();
      else detailReplaceInputRef.current?.focus();
    }, 0);
  };
  window.addEventListener("keydown", onKey, { capture: true });
  return () => window.removeEventListener("keydown", onKey, { capture: true });
}, [editingDetailTitle, detailSearchQuery]);
```

聚焦智能分流：query 空 → focus search 让 owner 先填关键词；query
非空 → 直接 focus replace 让 owner 立即填替换文本（流水线场景）。

#### 3. Replace 操作 helpers

```ts
const replaceMatchInContent = (content, start, end, replaceText) =>
  content.slice(0, start) + replaceText + content.slice(end);

const handleDetailReplaceCurrent = () => {
  if (matches.length === 0) return;
  const m = matches[safeIdx];
  setEditingDetailContent(replaceMatchInContent(content, m.start, m.end, replaceText));
  // activeIdx 不动：matches useMemo 重算后原位置上的下一条命中接班
};

const handleDetailReplaceAll = () => {
  let next = content;
  // 从后往前 splice 避免前面切换让后面位置漂移
  for (let i = matches.length - 1; i >= 0; i--) {
    const m = matches[i];
    next = replaceMatchInContent(next, m.start, m.end, replaceText);
  }
  setEditingDetailContent(next);
  setDetailSearchActiveIdx(0);
};
```

设计要点：
- **activeIdx 替换后不重置**：matches 数组重算后原 idx 的下一条命
  中自然接班（除非 replaceText 含 query 子串），让 Enter 连按推进
- **Replace All 反向遍历**：从尾 splice 防 head 切换影响 tail 位
  置 — string indexing 在变化时易出 off-by-one；反向是惯用稳定模式
- **复用既有 setEditingDetailContent 路径**：与既有 ⌘/ comment
  toggle / markdown toolbar 等同入口，自动触发 dirty check / draft
  autosave / matches useMemo 重算

#### 4. UI: find bar 加 replace row + 切换按钮

外层 div 改 flex-col；既有 find row 包内层 flex-row；新增 replace
row 在 `detailReplaceMode === true` 时渲染。Find row 加 ↳ 切换按钮
（blue tint 当 active）让 owner 不必依赖键盘也能切到 replace 半边。

Replace row：

```jsx
<div style={{ display: "flex", gap: 6, alignItems: "center" }}>
  <span>↳</span>
  <input
    ref={detailReplaceInputRef}
    value={detailReplaceText}
    onChange={...}
    onKeyDown={(e) => {
      if (e.key === "Escape") { close + refocus textarea; }
      if (e.key === "Enter") {
        e.preventDefault();
        if (e.metaKey || e.ctrlKey) handleDetailReplaceAll();
        else handleDetailReplaceCurrent();
      }
    }}
    placeholder="替换为…（Enter 单次 · ⌘Enter 全部 · 留空 = 删除命中 · Esc 关）"
  />
  <button onClick={handleDetailReplaceCurrent} disabled={n === 0}>替换</button>
  <button onClick={handleDetailReplaceAll} disabled={n === 0}>全部替换</button>
</div>
```

placeholder 文案点明三个 affordance（Enter / ⌘Enter / 留空删除）—
让 owner 第一次开就知道高频操作怎么按。

## Key design decisions

- **case-insensitive match（复用既有 useMemo 行为）**：与 find 一致；
  replace text 是字面量（无 case preservation）— owner 想保留原 case
  需自行多次替换或直接靠 textarea 手改。VSCode 默认也是字面替换。
- **不引入 regex 模式**：当前 find 是 substring；regex 是大半个 IDE
  feature，超本 iter 范围；典型场景（typo / 术语统一）字面就够
- **不抽 SearchReplaceBar 组件**：内联结构与 PanelTasks 其它本地 IIFE
  风格一致（既有 find bar 也是 inline IIFE 200 行）；抽出去要 8+
  props 不划算
- **Replace 半边内不另显 count chip**：左侧 layout 占位让两按钮纵
  向对齐 find row 的 count；count 信号已在 find row 上方，不重复
- **留空 replaceText 语义 = 删除**：与所有主流编辑器 (VSCode / IntelliJ
  / Notion) 一致 — owner 想删全部命中时不必绕路敲 sed
- **⌘Enter 全部替换 vs 单独按钮**：VSCode 风（⌘Enter 全部）+ 显式
  按钮兜底（鼠标党 / 键盘党都覆盖）
- **不为单 popover UI 引 unit test**：纯 setState + string splice
  helper；build pass + 手测足够：（1）⌘F 进 find → ↳ 按钮切 replace
  →（2）⌘⇧F 直接进 search+replace →（3）输 query + replaceText →
  Enter 单次替换看活动 match 替换 + activeIdx 推进 →（4）⌘Enter 全
  部替换看 count 归 0 + 内容批量更新 →（5）Esc 关回 textarea 焦点

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.23s)
- 后端无改动
