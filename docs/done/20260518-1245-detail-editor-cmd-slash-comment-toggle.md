# detail.md 编辑器「⌘/ markdown 注释行 toggle」shortcut（iter #473）

## Background

detail.md 编辑器 IDE 行操作集群已含 ⌘D 复制 / ⌘L 选中 / ⌘⇧K 删除 /
⌘⇧X 剪切 / ⌥↑↓ 移动 / ⌘⌥↑↓ 复制上下 — 但缺最后一个 IDE 标配 行操作：
**comment toggle**。owner 想临时藏一段 detail.md 内容（write-only safe
草稿 / TODO 标记 / "稍后再放回"等）时要手动敲 `<!-- ... -->`。

本 iter 加 ⌘/ — 当前行（无选区）或多行选区每行包 / 剥 markdown HTML
注释。

## Changes

### `src/components/panel/PanelTasks.tsx`

#### 1. `handleDetailCommentToggle` handler

紧贴 `handleDetailCutLine` 之后：

```ts
const handleDetailCommentToggle = useCallback((e) => {
  if (!(e.metaKey || e.ctrlKey)) return false;
  if (e.shiftKey || e.altKey) return false;
  if (e.key !== "/") return false;
  if ((e.nativeEvent as KeyboardEvent).isComposing) return false;
  const ta = e.currentTarget;
  const start = ta.selectionStart ?? 0;
  const end = ta.selectionEnd ?? start;
  const value = ta.value;
  const firstLineStart = value.lastIndexOf("\n", start - 1) + 1;
  const nextNl = value.indexOf("\n", end);
  const lastLineEnd = nextNl === -1 ? value.length : nextNl;
  e.preventDefault();
  const block = value.slice(firstLineStart, lastLineEnd);
  const lines = block.split("\n");
  const commentRe = /^<!--\s?(.*?)\s?-->$/;
  const nonEmpty = lines.filter((l) => l.length > 0);
  const allCommented = nonEmpty.length > 0 && nonEmpty.every((l) => commentRe.test(l));
  const transformed = lines.map((l) => {
    if (l.length === 0) return l;
    if (allCommented) {
      const m = commentRe.exec(l);
      return m ? m[1] : l;
    } else {
      if (commentRe.test(l)) return l;  // 已 commented 不二次包
      return `<!-- ${l} -->`;
    }
  });
  // ... 替换 + 光标 / 选区恢复
}, []);
```

#### 行为：uniform toggle

- **所有非空行已 commented** → 全部 uncomment（剥 `<!-- ` 前缀 + ` -->`
  后缀）
- **混合 / 未 commented** → 全部 comment（已 commented 行保持不变避
  免二次包 → `<!-- <!-- x --> -->`）
- **空行跳过**：保 newline 结构不被 `<!--  -->` 污染

#### 光标 / 选区恢复

- **单行（start === end）**：cursor 落原位置相对偏移（行扩 / 缩对应
  字符数）— 让连续 ⌘/ 多次切换状态时 cursor 不漂
- **多行选区**：保整段「first 行首 → 新 last 行尾」的选区，便于连续
  按 ⌘/ 切换状态

#### 2. 接入两 textarea onKeyDown 链

split + 纯 edit 模式两条 chain 都在 `handleDetailCutLine` 之后紧贴
插：

```ts
if (handleDetailCutLine(e)) return;
// ⌘/ markdown 注释行 toggle — IDE 标准 line-op
if (handleDetailCommentToggle(e)) return;
```

replace_all 单 match 同步两 chain。

## Key design decisions

- **uniform toggle 而非 per-line independent toggle**：VS Code / Sublime
  默认 per-line（混合状态下每行独立反转）；但 owner 心智「我选了 5
  行，按⌘/ 应该全部 commented OR 全部 uncommented」更直觉。每次按
  ⌘/ 状态稳定明确（"全 on 或 全 off"），连续按 toggle 同一组行不
  会让某些行偏离
- **空行跳过 wrap**：`<!--  -->` 在空行上没语义，反而破坏空行作为段
  落分隔的视觉。空行不计入 allCommented 判定，让"一段含空行的连续文
  本"toggle 仍按非空行的状态决定
- **已 commented 行不二次包**：comment 时跳过已 wrapped 的；防 `<!-- <!--
  x --> -->` 嵌套 hell
- **commentRe `^<!--\s?(.*?)\s?-->$`**：`\s?` 选择性匹配 wrap 时插的
  单空格；让历史手敲的 `<!--x-->`（无空格）或 `<!-- x -->`（含空格）
  都能 uncomment 干净
- **⌘/ 而非 ⌘K /**：⌘/ 是 VS Code / Sublime / JetBrains 通用单键
  shortcut；⌘K / 是 multi-key sequence 学习曲线更高。⌘/ 在 Chrome 的
  "search keyboard shortcut" 默认被 preventDefault 吃掉，Tauri webview
  无影响
- **不引 fenced ` ``` ` 代码注释 / `<!-- block -->`**：markdown 没原
  生「single-line」comment；`<!--` HTML 注释是约定俗成的 markdown
  注释方式。fenced 代码块注释要专用语言扩展（如 Prettier-ignore）—
  与本 chip "通用 line comment" 语义不符
- **不写 unit test**：纯字符串处理 + DOM textarea 副作用；逻辑
  trivial（既有 handleDetailDeleteLine boundary 算法 production 验
  证）。GOAL.md "meaningful tests only" 规则下不引装饰性测试。`tsc +
  vite build` clean 即够

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.34s)
- 后端无改动 — 纯前端 keyboard handler
- 手测：detail.md 编辑 → 光标停一行 → ⌘/ → 行被 `<!-- ... -->` wrap →
  再 ⌘/ → 行恢复原样；多行选区 → ⌘/ → 所有非空行同时 wrap → 再 ⌘/
  → 全部 uncomment；连续按 ⌘/ 反复 toggle 状态稳定
