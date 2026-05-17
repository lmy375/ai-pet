# ChatPanel 输入框加「📋 paste-as-plain」按钮（iter #487）

## Background / scope clarification

TODO 写「ChatMini 输入框加 📋 paste-as-plain 按钮」— 但 ChatMini 本
身不含 chat-message input box（仅 messages 列 + 右键 ctx menu + 搜索
框等）。实际 chat 输入框在 ChatPanel（render 与 ChatMini 同 sibling
位置 — App.tsx:1731）。

按 spec 精神实现：ChatPanel 的 textarea 加 paste-as-plain 入口（与
既有 detail.md ⌘⇧V 同 normalize 规则）。owner 从 Word / Notion /
飞书等 rich-text 源粘贴时去 unicode artifacts 污染。

## Changes

### `src/components/ChatPanel.tsx`

紧贴既有 💡 history 按钮之前插 📋 paste-as-plain 按钮（absolute 在
textarea 右上角，right: 36，让 💡 在 right: 8 各居其位不重叠）：

```tsx
<button
  onMouseDown={(e) => e.stopPropagation()}
  onClick={async (e) => {
    e.stopPropagation();
    let text: string;
    try {
      text = await navigator.clipboard.readText();
    } catch (err) {
      console.error("paste-as-plain read clipboard failed:", err);
      return;
    }
    if (text.length === 0) return;
    const clean = text
      .replace(/[“”]/g, '"')
      .replace(/[‘’]/g, "'")
      .replace(/ /g, " ")  // U+00A0 NBSP
      .replace(/[​‌‍﻿]/g, "")  // U+200B / U+200C / U+200D / U+FEFF zero-width
      .replace(/[–—]/g, "-");  // U+2013 en-dash / U+2014 em-dash
    const ta = textareaRef.current;
    if (!ta) {
      setInput((prev) => prev + clean);
      return;
    }
    const start = ta.selectionStart ?? input.length;
    const end = ta.selectionEnd ?? start;
    const next = input.slice(0, start) + clean + input.slice(end);
    setInput(next);
    requestAnimationFrame(() => {
      const cur = textareaRef.current;
      if (!cur) return;
      cur.focus();
      const newCursor = start + clean.length;
      cur.selectionStart = cur.selectionEnd = newCursor;
    });
  }}
  title="paste-as-plain：从剪贴板读文本 + normalize（smart quotes → ASCII / NBSP → 普通空格 / 零宽字符 → 删除 / em-dash → ASCII -）后插入光标位置。"
  aria-label="paste as plain text"
  style={{ position: "absolute", top: 6, right: 36, width: 22, height: 22, ... }}
>
  📋
</button>
```

### Normalize rules（与 detail.md ⌘⇧V 同 source-of-truth）

| Unicode source | → ASCII / normalized |
|---|---|
| U+201C / U+201D（"smart" quotes）| `"` |
| U+2018 / U+2019（'smart' quotes）| `'` |
| U+00A0（NBSP）| `<space>` |
| U+200B / U+200C / U+200D / U+FEFF（zero-width 系列）| 删除 |
| U+2013 / U+2014（en/em dash）| `-` |

不影响中文标点 / emoji / 既有 ASCII / 既有 ref token / markdown
syntax — 与 detail.md handler 同 set of replacements。

## Key design decisions

- **挂 ChatPanel 而非 ChatMini**：ChatMini 是 messages-only 容器；chat
  input 实际在 ChatPanel。TODO 题面 spec 与实际架构 mismatch — 按精
  神实现（chat 输入流的 paste-as-plain 入口）
- **复用同 normalize 规则集**：与 detail.md handleDetailPastePlainText
  完全同 regex 链。两 surface 行为一致；未来加 normalize rule 时改
  两处但语义同步（如有需要可后续抽 utils）
- **button 而非 ⌘⇧V keyboard**：ChatPanel input 已有 native paste
  handler 处理 image blobs；加 ⌘⇧V hijack plain paste 会冲突且降级
  native UX。button 是显式入口 — owner 想 clean paste 时点；想 raw
  paste 时按 native ⌘V
- **right: 36 位置**：💡 history 按钮（right: 8）保持原位；📋 在其左
  侧 32px。两按钮各 22x22 圆形 + 6px gap 视觉舒适。owner 一眼区分（💡
  是历史输入回看，📋 是粘贴 normalize）
- **clipboard 读失败 silent fallback**：console.error 不阻塞 — owner
  可走 native ⌘V 兜底（在 Tauri webview 权限 / API 不可用时）
- **不在 input 上 setSelection / 不破坏 history mode**：通过 setInput
  + ref.selectionStart 双路径写；onChange 会清掉 historyCursorRef
  （既有逻辑）— 但 clipboard 插入是"真用户输入"语义，清掉 history
  cursor 是正确行为
- **不写 unit test**：纯 clipboard read + 5 行 regex + textarea DOM 副
  作用；逻辑 trivial（既有 handleDetailPastePlainText 同算法 production
  验证）。GOAL.md "meaningful tests only" 规则下不引装饰性测试

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.29s)
- 后端无改动 — 纯前端 input 增强
- 手测：从 Word / Notion / GitHub 复制含 smart quotes / em dash / NBSP
  的文本 → ChatPanel 输入框 click 📋 按钮 → 文本插入光标位置含
  normalized ASCII 等价；rich-text 副作用全清；既有 💡 history 按钮
  位置 / 行为不变
