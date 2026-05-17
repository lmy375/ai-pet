# detail.md 编辑器 ⌘/ 切换 markdown 注释（iter #375）

## Background

detail.md 编辑器已有 IDE-style 行操作集群（⌘D 复制行 / ⌘L 选中行
/ ⌘⇧K 删除行 / Tab 缩进 / ⌘B / ⌘I），但缺 markdown 注释切换。owner
"暂时屏蔽段落"、"先备注一段试错版本"等场景目前要手敲 `<!--` `-->`
+ 删除。本 iter 加 ⌘/ 一键 toggle，与 VSCode markdown 习惯一致。

## Changes

### `src/components/panel/PanelTasks.tsx`

#### 1. capture-phase keydown useEffect（~line 2300，⌘P preview toggle 之前）

```ts
useEffect(() => {
  if (editingDetailTitle === null) return;
  const onKey = (e: KeyboardEvent) => {
    if (!(e.metaKey || e.ctrlKey)) return;
    if (e.shiftKey || e.altKey) return;
    if (e.key !== "/") return;
    if (document.activeElement !== detailEditorRef.current) return;
    e.preventDefault();
    e.stopImmediatePropagation();
    // ... toggle logic ...
  };
  window.addEventListener("keydown", onKey, { capture: true });
  return () => window.removeEventListener("keydown", onKey, { capture: true });
}, [editingDetailTitle]);
```

capture phase + stopImmediatePropagation 避免全局 ⌘/ 速查 modal
binding（useTaskKeyboardNav.ts:155）抢键。`activeElement` gate
让 textarea 焦点外的 ⌘/ 仍走全局速查 modal。

#### 2. toggle 算法

**操作 range**：
- 无选区 → 当前行整行（lastIndexOf "\\n" → 行首；indexOf "\\n" → 行尾）
- 有选区 → 原选区 [start, end]

**判断已包裹**：segment trim 后 startsWith `<!--` && endsWith `-->`。

**解注释**：保留 leading/trailing whitespace（缩进 / 换行 不被吞）；
strip leading `<!--` + 可选 inner space + trailing `-->` + 可选
inner space。容忍紧贴 `<!--foo-->` 与标准 `<!-- foo -->` 两种风格。

**包裹**：在 trim 后内容前加 `<!-- `，后加 ` -->`；leading / trailing
whitespace 保留。

**全空选区 / 空行 → noop**：trim 后 length === 0 时直接返回。

#### 3. 光标 / 选区调整（rAF 内）

- 无选区 → 光标落 replacement 末尾
- 有选区 → 选区覆盖新 replacement（让 owner 再次 ⌘/ 反向 toggle）

#### 4. placeholder + cheatsheet

- 两 textarea placeholder 加 "⌘/ markdown 注释"
- 速查 modal 增条目 `["⌘/", "切换 markdown 注释 <!-- … --> ..."]`，
  位置紧贴 Tab/⇧Tab 行（IDE-cluster 同段）

## Key design decisions

- **capture phase 拦截 vs 与全局 ⌘/ 共存**：textarea 焦点内 owner
  几乎不会想"toggle 速查 modal"（既然在 typing 上下文）。textarea
  焦点外 ⌘/ 仍走全局 — 完美职责分离。
- **block comment 而非每行单独 wrap**：VSCode markdown extension 默认
  block comment（`<!-- a\\nb -->`）— 与之对齐让 cross-editor 心智一
  致。如未来用户要"per-line wrap"再加 ⌘⇧/ 等价键路径。
- **容忍紧贴 / 带 space 两种风格**：解注释时检测 inner space pad —
  既能 round-trip 标准格式，又能解掉外部工具写的紧贴格式。包裹时
  统一输出标准 ` ` pad（更可读）。
- **保留 leading/trailing whitespace**：行缩进 `  - bullet` 包裹后
  应是 `  <!-- - bullet -->` 不是 `<!--   - bullet -->`。trim 后包
  裹是关键 — 不破坏 indent 信号。
- **operate on 整行而非 cursor 位置（无选区时）**：与 ⌘D 复制行 /
  ⌘L 选中行 / ⌘⇧K 删除行 同行操作集群心智 — "无选区 → 当前行"
  贯穿。VSCode markdown comment 同模式。
- **不为单 fn 引 unit test runner**：项目无 .test.tsx 历史；
  行为是 IO + state ops，build pass + 手测足够（包裹 / 解注释 /
  缩进保留 / 多行选区 四个场景手测一遍）。

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.24s)
- 后端无改动
