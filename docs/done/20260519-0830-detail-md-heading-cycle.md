# detail.md 编辑器加「⌘⇧H heading-level cycle」shortcut（iter #548）

## Background

markdown 写作常需要快速调整段落 heading level — 「这段升级为 H2」/
「这段降为 H3」/「不再是 heading 改回普通段落」。手敲 `#` 漏 / 多容
易；行内 cursor 移动 + del + 重输噪音多。

本 iter 加 **⌘⇧H** — 当前行 heading level 循环切换。

## Cycle 状态机

```
none ──── # ────► h1
  ▲                │
  │                # ──► h2
  │                       │
  │                       # ──► h3
  │                              │
  │                              # ──► h4
  │                                     │
  └─────────────── ── remove ◄──────────┘
```

`#### → none` 而非继续 h5 / h6 — owner 实际用 ≥5 级少；让 cycle 闭环
（5 步循环回原态）。已是 h5/h6 行（极少见）也走 → none 让 reset 可达。

## Changes

### `src/components/panel/PanelTasks.tsx`

新 `handleDetailHeadingCycle` callback（紧贴 `handleDetailBlockquote`
之后）：

```tsx
const handleDetailHeadingCycle = useCallback(
  (e: React.KeyboardEvent<HTMLTextAreaElement>): boolean => {
    if (!(e.metaKey || e.ctrlKey)) return false;
    if (!e.shiftKey || e.altKey) return false;
    if (e.key.toLowerCase() !== "h") return false;
    if ((e.nativeEvent as KeyboardEvent).isComposing) return false;
    e.preventDefault();
    const ta = e.currentTarget;
    const start = ta.selectionStart ?? 0;
    const value = ta.value;
    // 扩边到当前行（仅 start 边）
    const lineStart = value.lastIndexOf("\n", start - 1) + 1;
    const nextNl = value.indexOf("\n", lineStart);
    const lineEnd = nextNl === -1 ? value.length : nextNl;
    const line = value.slice(lineStart, lineEnd);
    // 探测 leading `#{1,6} `
    const m = /^(#{1,6}) (.*)$/.exec(line);
    let newLine: string;
    let cursorOffset = 0;
    if (m) {
      const hashes = m[1];
      const rest = m[2];
      const lv = hashes.length;
      if (lv >= 4) {
        // h4/h5/h6 → none (回到普通段落)
        newLine = rest;
        cursorOffset = -(hashes.length + 1);
      } else {
        // h1/h2/h3 → 下一级
        newLine = `${"#".repeat(lv + 1)} ${rest}`;
        cursorOffset = 1;
      }
    } else {
      // 无 heading prefix → 加 `# `（h1）
      newLine = `# ${line}`;
      cursorOffset = 2;
    }
    setEditingDetailContent(before + newLine + after);
    const newCursor = Math.max(lineStart, start + cursorOffset);
    requestAnimationFrame(() => {
      // cursor 重置 + state sync
    });
    return true;
  },
  [],
);
```

#### 接入 onKeyDown 链

两个 textarea（split / edit-only）都接入。

#### Keyboard help modal

```tsx
["⌘⇧H", "当前行 heading level 循环（none→h1→h2→h3→h4→none）"],
```

## Key design decisions

- **仅 cursor 所在行 — 不批量**：批量场景走选区 + 多次 ⌘⇧H（cursor
  下移）— 简化算法 + 避免 multi-line cursor 推算复杂度。批量重排走
  ⌘⌥L sort-lines 系列
- **h4 → none 而非 h5/h6**：owner 实际用 ≤ h4 级；让 cycle 5 步闭环
  比开放无限级更可预测。已是 h5/h6 行也走 → none 让 reset 可达
- **`#{1,6} ` regex** with mandatory space**：CommonMark spec — heading
  必须 `#` 后接 1 个空白才算 heading；`####foo` 是普通文本。本 helper
  按 spec 处理
- **cursor 维持相对位置**：lv=1→2 时 prefix +1 char（cursor +1）；
  4→none 时 prefix - (lv+1) chars（cursor 相应减）— 让 owner 输内容
  时 cursor 不跳到行首
- **Math.max(lineStart, ...) 防越界**：cursor 减后可能 < lineStart
  （极端：cursor 在 prefix 中间），clamp 保 lineStart
- **modifier ⌘⇧H**：⌘H 是 macOS hide app / browser history；shift
  修饰避开。⌘⇧H 在 IDE mostly 空 — "Heading" 助记
- **不写 unit test**：纯 regex + 字符串拼接 + cursor 算术；逻辑 trivial
  （既有 sort-lines / blockquote / move-lines 同 line-op pattern
  production 验证）。GOAL.md "meaningful tests only" 规则下不引装饰
  性测试

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.40s)
- 后端无改动 — 纯前端 keyboard shortcut
- 手测：
  - 普通行 `这是一段内容` → ⌘⇧H → `# 这是一段内容`
  - 再按 ⌘⇧H → `## 这是一段内容`
  - 再按 ⌘⇧H → `### 这是一段内容`
  - 再按 ⌘⇧H → `#### 这是一段内容`
  - 再按 ⌘⇧H → `这是一段内容`（回到 none）
  - 已是 `##### foo`（h5）→ ⌘⇧H → `foo`（h5 也走 → none）
  - cursor 在行中段 → ⌘⇧H → cursor 相对位置维持（用户继续输内容自然）
  - 跨 split / edit-only 模式都触发
  - ⌘/ 帮助 modal 看到「⌘⇧H」行

## Future iters (out of scope)

- 「⌘⇧⌥H 反向 cycle」— none→h4→h3→h2→h1→none；按需 propose
- 「⌘⇧Number」直接跳 level — ⌘1/⌘2/⌘3 等占键易冲突浏览器 tab 切换
- 批量 wrap as heading（与 ⌘⇧Q blockquote 批量同模板）— 后续按需
