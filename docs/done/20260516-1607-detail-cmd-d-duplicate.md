# detail.md textarea ⌘D 复制当前行

## 背景

TODO 上 auto-proposed 一条："detail.md textarea ⌘D 复制当前行：与 IDE / Sublime ⌘D 同直觉，光标所在行复制粘一份在下方。"

Sublime / JetBrains / Cursor 等 IDE 都把 ⌘D 绑定为 "复制当前行" —— owner 在 detail.md 写代码片段 / list item / table row 时常想"再来一行同样的"。手动 ⌘← ⇧⌘→ ⌘C ⌘V ⌘V 五步太烦。补一个 ⌘D 一步到位。

（VSCode 是 ⌘D = "select next occurrence"，与本文不冲突 —— 本 iter 仅作用 detail.md textarea；其它编辑场景无）。

## 改动

### `src/components/panel/PanelTasks.tsx`

#### `handleDetailDuplicateLine` useCallback

紧贴既有 `handleDetailBracketPair` 之后插入（在 `handleDetailListContinue` 之前的位置）。返回 boolean。

```ts
const handleDetailDuplicateLine = useCallback(
  (e: React.KeyboardEvent<HTMLTextAreaElement>): boolean => {
    if (!(e.metaKey || e.ctrlKey)) return false;
    if (e.shiftKey || e.altKey) return false;
    if (e.key.toLowerCase() !== "d") return false;
    if ((e.nativeEvent as KeyboardEvent).isComposing) return false;
    const ta = e.currentTarget;
    const start = ta.selectionStart ?? 0;
    const end = ta.selectionEnd ?? start;
    const value = ta.value;
    e.preventDefault();
    if (start !== end) {
      // 选区非空：在选区末尾插一份相同文本，新副本仍 selected 让 owner 可
      // 连按 ⌘D 累积粘多份。
      const selected = value.slice(start, end);
      const next = value.slice(0, end) + selected + value.slice(end);
      const newSelEnd = end + selected.length;
      setEditingDetailContent(next);
      setDetailCursorPos(end);
      requestAnimationFrame(() => {
        const t = detailEditorRef.current;
        if (!t) return;
        t.focus();
        t.selectionStart = end;
        t.selectionEnd = newSelEnd;
      });
      return true;
    }
    // 空选区：复制光标所在整行到下一行；光标落到新行相对 column。
    const lineStart = value.lastIndexOf("\n", start - 1) + 1;
    const lineEnd = value.indexOf("\n", start);
    const lineEndIdx = lineEnd === -1 ? value.length : lineEnd;
    const lineText = value.slice(lineStart, lineEndIdx);
    const insertion = `\n${lineText}`;
    const next = value.slice(0, lineEndIdx) + insertion + value.slice(lineEndIdx);
    const colOffset = start - lineStart;
    const newCursor = lineEndIdx + 1 + colOffset;
    setEditingDetailContent(next);
    setDetailCursorPos(newCursor);
    requestAnimationFrame(() => {
      const t = detailEditorRef.current;
      if (!t) return;
      t.focus();
      t.selectionStart = t.selectionEnd = newCursor;
    });
    return true;
  },
  [],
);
```

#### 两 textarea onKeyDown 接入

split + preview-fallback edit 两 textarea onKeyDown 在 bracket-pair handler 之后加：

```ts
if (handleDetailDuplicateLine(e)) return;
```

## 关键设计

- **两态语义**：
  - **选区非空**：复制选中文本插在选区末尾，新副本仍 selected → 连按 ⌘D 可累积粘 2/4/8 份（Sublime mode）。
  - **选区空**：复制光标所在整行到下方，光标落到**新行的同 column** 位置 → 与 JetBrains 模式一致。新行光标继承 column 让 owner 可继续编辑（e.g., 复制 "- foo" 后光标自然在 "- foo" 末尾位置同列，按 Backspace 删 foo 就能改成不同 item）。
- **严格 `!shift && !alt`**：⌘⇧D 留给"删除当前行"语义（后续 iter 可加）；⌘⌥D 留给"向上复制"等扩展。
- **IME composing 跳过**：与 bracket pair / list continue 同 guard，输入法候选阶段不抢键。
- **`preventDefault`**：browser 默认 ⌘D 是 "Add bookmark"，Tauri webview 通常不弹但 preventDefault 兜底安全。
- **`rAF + setDetailCursorPos`**：与既有 bracket pair / list continue 同模式 —— setState 后 rAF 才设 selection 避免 React 重渲覆盖。底部状态栏行号 chip 同步刷新。
- **不嵌入 bracket / list 同一 handler**：三个 handler 都是独立特化，各管一类语义。如果以后扩 ⌘D / ⌘L / ⌘⇧K 等更多 IDE 行为，分立 handler 各自小、可独立测试 / 移除。
- **优先级在 bracket pair 之后、⌘S 之前**：bracket pair 是字符级 intercept（与 modifier 无关），先处理；⌘D 是 modifier+key，与 ⌘S 同层级，并列。三者互不冲突，顺序仅为代码可读 / 防未来 modifier 扩展时的歧义。

## 不做

- **不支持 ⌘⇧D 删除当前行**：scope creep。如果 owner 想删某行用 ⌘A 选行 / Backspace 即可。
- **不写测试**：纯 keydown + textarea selection + setState；既有 bracket pair / list continue 都视觉验证。手动测：写一行 `- 倒垃圾`，光标停某 col，⌘D → 看到第二行复制 + cursor 落新行同 col。
- **不接桌面 ChatMini / PanelChat textarea**：chat 输入场景不期待 ⌘D 复制 message 草稿。本 iter 专注 detail.md 长文写作场景。
- **不持久化 disable 开关**：⌘D 是 IDE 通用便利，全开默认。如果有用户不想要可走 Shift+⌘D 等组合（当前未占用）绕开。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.19s
- 改动 ~70 行（handleDetailDuplicateLine useCallback 60 + 两 textarea 1 行接入 + 注释）；既有 bracket pair / list continue / ⌘S / Esc / setEditingDetailContent / setDetailCursorPos 路径完全不动。

## TODO 状态

6 条 auto-proposed 已完成 2 条，余 4 条留池：
- 桌面 pet 主区顶部 "今天 N 条 · 主动 M 次" ambient
- PanelSettings 🔌 测试 LLM 连通性按钮
- proactive prompt 加最近 24h 完成任务
- PanelChat sort chip

## 后续

- ⌘⇧D 删除当前行：补完"复制 / 删除"两个 IDE 对偶。
- ⌘⌥↑ / ⌘⌥↓ 上下移动当前行：JetBrains / VSCode 通用快捷键。
- Cursor 跨多行选中时按 ⌘D 复制每行：当前选区跨多行也是按文本 slice 处理，行为可接受。多光标 ⌘D（Sublime "select next occurrence"）超出本 iter 范围。
