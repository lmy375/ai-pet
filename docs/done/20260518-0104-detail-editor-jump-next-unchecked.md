# detail.md 编辑器 ⌘] 跳下一 unchecked checklist shortcut（iter #437）

## Background

owner 在长 detail.md（数百行 GFM checklist）想找下一个未完成
`- [ ]` 项 — 当前只能滚屏肉眼扫，或用 ⌘F 搜「- [ ]」（但要敲字
+ 按 Enter 跳）。

本 iter 加 ⌘] 单键跳到下一个 unchecked checklist 行；无命中时
wrap 从头扫一遍（让 owner 不必滚回顶手动找）；全无 unchecked 时
noop。⌘[ 保留给浏览器默认（前进 / 后退），单方向已够 audit。

## Changes

### `src/components/panel/PanelTasks.tsx`

#### 1. `handleDetailJumpNextUnchecked` callback

```ts
const handleDetailJumpNextUnchecked = useCallback(
  (e: React.KeyboardEvent<HTMLTextAreaElement>): boolean => {
    if (!(e.metaKey || e.ctrlKey)) return false;
    if (e.shiftKey || e.altKey) return false;
    if ((e.nativeEvent as KeyboardEvent).isComposing) return false;
    if (e.key !== "]") return false;
    e.preventDefault();
    const ta = e.currentTarget;
    const value = ta.value;
    const cursor = ta.selectionStart ?? 0;
    const re = /(^|\n)([ \t]*)- \[ \] /g;
    let foundIdx = -1;
    let m: RegExpExecArray | null;
    re.lastIndex = cursor;
    while ((m = re.exec(value)) !== null) {
      const lineStart = m.index + m[1].length;
      if (lineStart > cursor) { foundIdx = lineStart; break; }
    }
    if (foundIdx < 0) {
      // wrap 从头扫
      re.lastIndex = 0;
      while ((m = re.exec(value)) !== null) {
        const lineStart = m.index + m[1].length;
        if (lineStart <= cursor) { foundIdx = lineStart; break; }
      }
    }
    if (foundIdx < 0) return true; // 全无 — noop（不动 cursor）
    requestAnimationFrame(() => {
      const cur = detailEditorRef.current;
      if (!cur) return;
      cur.focus();
      cur.selectionStart = cur.selectionEnd = foundIdx;
      setDetailCursorPos(foundIdx);
      setDetailSelectionEnd(foundIdx);
    });
    return true;
  },
  [],
);
```

设计要点：
- **regex `(^|\n)([ \t]*)- \[ \] `**：行起始 OR `\n` boundary +
  可选 indent + `- [ ] ` literal（含尾空格防匹配 `- [ ]]` 异常）。
  match group 1 是 `^` 空串 / `\n` — `lineStart = m.index + m[1].length`
  跨过 `\n` 落到实际行首位置
- **空 spec / 已 checked 跳过**：`- [x] ` / `- [X] ` 已完成不命中；
  `- []`（缺空格）也不命中 — 严格 GFM 协议
- **wrap-around 而非 lock-to-end**：到底 → 从头扫一遍。owner audit
  时不必滚回顶手动找下一个，与 IDE 「find next」wrap 同模式
- **全无 unchecked 仍 return true**：阻止默认行为（浏览器 ⌘]
  forward）但不动 cursor；防 owner 按错时跳乱
- **requestAnimationFrame 设 selection**：与既有 markdown shortcut
  helpers（ ⌘B/I/K/`）同模式 — 让 React 渲染先跑，避免 stale ref
- **IME composing guard**：与既有 shortcut 同 guard 防中文输入法
  组词时误触

#### 2. textarea 接入 dispatch chain（紧贴 ⌘\` 之后）

```tsx
if (handleDetailBoldItalic(e)) return;
if (handleDetailCodeBlock(e)) return;
if (handleDetailJumpNextUnchecked(e)) return;
```

## Key design decisions

- **不为 ⌘[ 加反向跳**：浏览器 ⌘[ 是 "back"，在 Tauri webview 多
  无 nav stack 也是 no-op；但让出给系统是稳妥默认。Audit 场景单
  方向跳已够（wrap-around 也能从末尾跳回头）
- **不显跳转位置反馈 toast**：textarea 自身的 selection focus 已
  足够视觉信号（光标 + textarea 自动滚到行）
- **regex 不复用既有 helper**：既有 GFM checklist 解析在 parseMarkdown
  中是渲染流；此处是 textarea 编辑流，行为不一样（一个 → DOM 节点，
  一个 → string offset），单 regex 直接表达更清晰
- **不引 cursor-line-aware 排除**：cursor 当前行也是 `- [ ] ` 时
  仍跳到下一条 —— 与 IDE「下一搜索结果」语义一致（按 ⌘] 多次会
  连续跳）
- **不为单 fn 引 unit test runner**：regex + setState；行为是 textarea
  cursor 移动；build pass + 手测足够（写一段含 5 个 `- [ ]` 的 checklist
  → 反复按 ⌘] 看光标依次跳到每个 → 到末尾再按 → 看 wrap 回到
  第一个 → 全勾选后按 → 看 noop 不动）

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.35s)
- 后端无改动 — 纯前端 regex + selection 移动
