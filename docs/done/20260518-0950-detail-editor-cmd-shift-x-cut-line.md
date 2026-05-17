# detail.md 编辑器「⌘⇧X 行剪切」shortcut（iter #465）

## Background

detail.md 编辑器既有 IDE 行操作集群：
- ⌘D — duplicate line / 复制行
- ⌘L — select line / 选中行
- ⌘⇧K — delete line / 删除行
- ⌥↑/⌥↓ — move lines / 上下移行
- ⌘⌥↑/⌘⌥↓ — copy lines / 复制行向上下

但缺一个：**cut line**（剪切行 = 删除 + 写剪贴板）。owner 想把一行 /
多行剪切到剪贴板搬到 detail.md 其它位置 / 别的笔记工具时，要"⌘L 选中
行 + ⌘X 系统剪切" 两步。

本 iter 加 ⌘⇧X 单步行剪切 — 与 ⌘⇧K 删除行同算法但额外写 clipboard。

## Changes

### `src/components/panel/PanelTasks.tsx`

#### 1. `handleDetailCutLine` handler

紧贴 `handleDetailDeleteLine` 之后：

```ts
const handleDetailCutLine = useCallback((e) => {
  if (!(e.metaKey || e.ctrlKey)) return false;
  if (!e.shiftKey || e.altKey) return false;
  if (e.key.toLowerCase() !== "x") return false;
  if ((e.nativeEvent as KeyboardEvent).isComposing) return false;
  const ta = e.currentTarget;
  const start = ta.selectionStart ?? 0;
  const end = ta.selectionEnd ?? start;
  const value = ta.value;
  const firstLineStart = value.lastIndexOf("\n", start - 1) + 1;
  const nextNl = value.indexOf("\n", end);
  const deleteUntil = nextNl === -1 ? value.length : nextNl + 1;
  e.preventDefault();
  const cut = value.slice(firstLineStart, deleteUntil);
  navigator.clipboard.writeText(cut).catch((err) => {
    console.error("cut line clipboard write failed:", err);
  });
  const next = value.slice(0, firstLineStart) + value.slice(deleteUntil);
  const newCursor = Math.min(firstLineStart, next.length);
  setEditingDetailContent(next);
  setDetailCursorPos(newCursor);
  requestAnimationFrame(() => {
    const t = detailEditorRef.current;
    if (!t) return;
    t.focus();
    t.selectionStart = t.selectionEnd = newCursor;
  });
  return true;
}, []);
```

复用 `handleDetailDeleteLine` 的 boundary 算法（firstLineStart /
deleteUntil 含行尾换行）— 删除行为完全一致；额外 step 是 `cut` 切片
写 clipboard。

#### 2. 两 textarea onKeyDown chain 接入

split + 纯 edit 模式两条 chain 都在 `handleDetailDuplicateLine` 之后
紧贴插入：

```ts
if (handleDetailDuplicateLine(e)) return;
// ⌘⇧X 行剪切（剪当前行 / 多行选区到剪贴板 + 删除）— 与 ⌘⇧K 删除
// 行互补，复用同 boundary 算法。
if (handleDetailCutLine(e)) return;
```

两 textarea 都需要本 shortcut，replace_all 单 string match 同时改两处。

## Key design decisions

- **⌘⇧X 选键**：VS Code / Sublime 「Cut Line (Empty Selection)」默
  认是 ⌘X（覆盖选区或无选区时整行）；但本 editor 的 ⌘X 是浏览器默认
  剪切选区（不抢，与 owner 习惯一致）。⌘⇧X 留作"显式整行剪切"快捷
  键，与 ⌘⇧K（删除行）同 modifier family 一致
- **clipboard 失败容忍**：与 PanelDebug 既有复制 chip 同 pattern —
  console.error 不阻断，**仍执行删除**。clipboard 失败常因权限 /
  非 focus / OS 限制，少见但要兜底。如果失败时不删，会让 ⌘⇧X 在 fail
  silent 时变成 ⌘⇧K 也丢——不直觉
- **复用 delete-line 算法**：boundary firstLineStart / deleteUntil
  含行尾 `\n` 的协议已 production 验证；rewriting 同算法多个变种是
  drift 风险。一处 const 计算两路径共享
- **不引入 toast 反馈**：行操作 cluster（⌘D / ⌘L / ⌘⇧K / ⌥↑↓ / ⌘⌥↑↓
  / ⌘⇧X）都无 toast — 视觉反馈靠 textarea 内容变化本身。加 toast
  会让 fast keyboard-driven owner 在密集行操作时被打扰
- **两 textarea chain 都接入**：split + edit 模式独立 onKeyDown chain，
  与 ⌘⇧C / ⌘⇧D 同处理 — replace_all 一次性同步避 drift
- **不写 unit test**：纯 keyboard handler + navigator.clipboard 副作
  用 + setEditingDetailContent；逻辑 trivial（既有 delete-line tests
  覆盖 boundary 计算）。GOAL.md "meaningful tests only" 规则下不引装
  饰性测试。`tsc + vite build` clean 即够

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.29s)
- 后端无改动 — 纯前端 keyboard handler
- 手测：detail.md 编辑 → 光标停在一行中 → ⌘⇧X → 行被删除（与 ⌘⇧K
  同 visual）+ 切到 markdown 编辑器 ⌘V 粘贴看到该行内容；split / pure
  edit 模式都生效；⌘D 复制 / ⌘L 选 / ⌘⇧K 删 / ⌘⇧X 剪 四象限完整
