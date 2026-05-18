# detail.md 编辑器加「⌘⇧Q blockquote wrap」shortcut（iter #538）

## Background

owner 写 detail.md 常需要把一段引用 / 注解 / 别人说的话 wrap 为 markdown
blockquote（每行 `> ` 前缀）。既有 markdown 编辑选项：

- ⌘\` fenced code block — code 段
- ⌘⇧A alert callout — `[!NOTE]` 风
- ⌘B/I 粗斜 wrap — inline
- ⌘⇧M table — 表格

但缺 **blockquote wrap** shortcut — 多行手敲 `> ` 前缀容易漏；选段后
没有一键 wrap 入口。

本 iter 加 ⌘⇧Q — 选区行 wrap 为 blockquote（每行加 `> ` 前缀；空行
用 `>`）。

## Changes

### `src/components/panel/PanelTasks.tsx`

新 `handleDetailBlockquote` callback（紧贴 `handleDetailAlertTemplate`
之后）：

```tsx
const handleDetailBlockquote = useCallback(
  (e: React.KeyboardEvent<HTMLTextAreaElement>): boolean => {
    if (!(e.metaKey || e.ctrlKey)) return false;
    if (!e.shiftKey || e.altKey) return false;
    if (e.key.toLowerCase() !== "q") return false;
    if ((e.nativeEvent as KeyboardEvent).isComposing) return false;
    e.preventDefault();
    const ta = e.currentTarget;
    const start = ta.selectionStart ?? 0;
    const end = ta.selectionEnd ?? start;
    const value = ta.value;
    // 扩边到整行（与 sort-lines / move-lines 同算法）
    const blockStart = value.lastIndexOf("\n", start - 1) + 1;
    const probe = end > start ? end - 1 : end;
    const nextNl = value.indexOf("\n", probe);
    const blockEnd = nextNl === -1 ? value.length : nextNl;
    const block = value.slice(blockStart, blockEnd);
    // 每行加 `> ` 前缀；空行用 `>`（无 trailing 空格 — markdown 渲染习惯）
    const wrapped = block.split("\n")
      .map((line) => (line.length === 0 ? ">" : `> ${line}`))
      .join("\n");
    if (wrapped === block) return true;  // 已 wrap 不变 — 防御幂等
    setEditingDetailContent(...);
    // 选区扩覆盖 wrapped block（与 sort-lines / move-lines 同 UX）
    ...
    return true;
  },
  [],
);
```

### 行扩展算法

与 ⌘⌥L sort-lines / ⌥↑↓ move-lines / ⌘⇧K delete-line 同：

```
blockStart = value.lastIndexOf("\n", start - 1) + 1
blockEnd = next "\n" after end OR value.length
```

让 owner 选半行覆盖时也按整行算。

### 空行处理

`line.length === 0 ? ">" : "> " + line` — 空行渲染 `>` 不带 trailing
空格。markdown spec：连续 `> ` 行（含 `>`）构成单 blockquote block；
空 `>` 行示段内空行 — 与既有 selection→blockquote copy helper（line
3015）同协议。

### Keyboard help

紧贴 `⌘⇧A`：

```tsx
["⌘⇧Q", "选区行 wrap markdown blockquote（每行 `> ` 前缀；空行用 `>`）"],
```

## Key design decisions

- **modifier ⌘⇧Q**：⌘Q 是 macOS quit app；shift 修饰避开。⌘⇧Q 在 IDE
  mostly 空 — 占给 "Quote" 助记
- **行级 wrap 不是 selection wrap**：blockquote 是 line-prefix 语义而
  非 inline wrap — 必须按行处理。复用 sort-lines / move-lines 既有
  行扩展算法保一致
- **空行用 `>` 不加空格**：与 markdown 渲染 / 既有 blockquote copy
  helper（line 3015）同协议 — 空 `>` 行示段内空行不破 quote block
- **idempotent short-circuit**：`wrapped === block` 时 return 不写
  state — 避免无变化时触发 dirty-flag flip
- **选区扩覆盖 wrapped block**：与 sort-lines / move-lines 同 UX；owner
  二次操作（如 ⌘⇧Q 后 ⌘B 加粗）可直接连续
- **不写 unit test**：纯 string split + map + join + textarea state
  set；逻辑 trivial（既有 sort-lines / move-lines 同 algorithm
  production 验证）。GOAL.md "meaningful tests only" 规则下不引装饰
  性测试

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.31s)
- 后端无改动 — 纯前端 keyboard shortcut
- 手测：
  - 选 3 行文本 → ⌘⇧Q → 3 行各加 `> ` 前缀；选区扩覆盖整 wrapped
    block
  - 选含空行的段 → 空行变 `>`，非空行变 `> <text>`
  - 无选区 + 光标在某行内 → 仅该行 wrap（半行覆盖按整行算）
  - 已是 `> text` 的行再 ⌘⇧Q → 变 `> > text`（嵌套 blockquote，预期）
  - 跨 split / edit-only 模式都触发
  - ⌘/ 帮助 modal 看到「⌘⇧Q」行

## Future iters (out of scope)

- 「⌘⇧⌥Q unwrap blockquote」反向 — 选区行剥 `> ` 前缀；与 ⌘/ comment
  toggle 同 mode
- 「nested blockquote depth limit」— 当前可无限嵌套；owner 触发误
  操作可走 ⌘Z
