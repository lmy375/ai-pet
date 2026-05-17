# detail.md 编辑器「⌘. checklist toggle」shortcut（iter #482）

## Background

detail.md GFM checklist (`- [ ] ` / `- [x] `) 是 owner 进度笔记最频
繁用例 — pet 写 daily review / owner audit 任务时常用。但既有路径要
切到 preview / split mode 找 checkbox 鼠标双击触发；键盘党在 textarea
编辑时 friction。

本 iter 加 ⌘. — 当前行 `- [ ] ` ↔ `- [x] ` 反转。键盘党不必离开
textarea / 不必精确定位 checkbox。

## Changes

### `src/components/panel/PanelTasks.tsx`

#### `handleDetailChecklistToggle` handler

紧贴 `handleDetailCommentToggle` 之后：

```ts
const handleDetailChecklistToggle = useCallback((e) => {
  if (!(e.metaKey || e.ctrlKey)) return false;
  if (e.shiftKey || e.altKey) return false;
  if (e.key !== ".") return false;
  if ((e.nativeEvent as KeyboardEvent).isComposing) return false;
  const ta = e.currentTarget;
  const start = ta.selectionStart ?? 0;
  const value = ta.value;
  const lineStart = value.lastIndexOf("\n", start - 1) + 1;
  const nextNl = value.indexOf("\n", start);
  const lineEnd = nextNl === -1 ? value.length : nextNl;
  const line = value.slice(lineStart, lineEnd);
  const m = /^(\s*- \[)([ xX])(\] )(.*)$/.exec(line);
  if (!m) return false;  // 严格模式：非 checklist 行 noop
  e.preventDefault();
  const newState = m[2] === " " ? "x" : " ";
  const newLine = `${m[1]}${newState}${m[3]}${m[4]}`;
  const next = value.slice(0, lineStart) + newLine + value.slice(lineEnd);
  setEditingDetailContent(next);
  setDetailCursorPos(start);
  requestAnimationFrame(() => {
    const t = detailEditorRef.current;
    if (!t) return;
    t.focus();
    t.selectionStart = t.selectionEnd = start;
  });
  return true;
}, []);
```

#### Regex `^(\s*- \[)([ xX])(\] )(.*)$`

- 4 个 capture group：indent + 前缀、state（" " / "x" / "X"）、`] ` 闭
  合 + 空格、rest
- 支持任意缩进（嵌套 list）`\s*`
- state 大小写都接（`x` / `X`），输出统一 `x`
- 严格模式：非 checklist 行 → return false → 不消费事件 → 让 native
  `.` 字符照常插入（owner 在打普通段落时 ⌘. 仍是浏览器/OS 的 default
  shortcut；本 handler 仅在 checklist 行 preventDefault + toggle）

#### 接入两 textarea onKeyDown chain

split + 纯 edit 模式都加 `if (handleDetailChecklistToggle(e)) return;`
紧贴既有 `handleDetailCommentToggle` 之后。replace_all 一次同步两 chain
避 drift。

## Key design decisions

- **严格模式 noop on non-checklist line**：不主动添加 `- [ ] ` 前缀 —
  避免破坏 owner 在打非 checklist 段落时的内容。想创建新 checklist 走
  既有 markdown toolbar 「• 列表」 / handleDetailListContinue 在 `- [ ]
  ` 末按 Enter 续 GFM checklist；本 shortcut 是「已有的 toggle」
- **保 cursor 在原位置**：行长度不变（` ` ↔ `x` 都是 1 char），cursor
  offset 不必调整。owner 可连续 ⌘. 反复 toggle 不漂
- **regex group 含 `\] ` 而非 `\]`**：要求 `]` 后有空格才视为 checklist
  — 兼容 markdown 严格语法（缺空格的 `- [ ]hi` 不是 valid checklist），
  避免误触 markdown plain bracket 内容
- **支持任意缩进 `\s*`**：嵌套 list `  - [ ] ` 也 toggle，与既有
  handleDetailListContinue 嵌套 checklist 支持一致
- **⌘. 选键**：键盘 1 键易触；浏览器无 default；Tauri webview 无冲突。
  与 ⌘B / ⌘I / ⌘U / ⌘`不重叠
- **不写 unit test**：纯 regex + 字符串切片 + DOM textarea 副作用；
  逻辑 trivial（既有 handleDetailCommentToggle 同 line-op 算法 production
  验证）。GOAL.md "meaningful tests only" 规则下不引装饰性测试

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.28s)
- 后端无改动 — 纯前端 keyboard handler
- 手测：detail.md 编辑 → 光标停在 `- [ ] 任务 A` 行 → ⌘. → 变 `- [x] 任务 A`
  → 再 ⌘. → 恢复 `- [ ] 任务 A`；非 checklist 行（如 `# heading` /
  `plain text` / `- 普通 list`）按 ⌘. → 不动作（textarea 默认插入 `.`）
