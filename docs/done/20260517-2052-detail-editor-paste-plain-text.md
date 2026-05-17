# detail.md 编辑器 ⌘⇧V 粘贴为纯文本（iter #391）

## Background

owner 从浏览器 / Notion / Slack 等 source copy 文本 paste 到 detail.md
时，文本表面看起来 plain，但实际含 unicode artifacts：smart quotes
(`"…"`、`'…'`)、NBSP（U+00A0，看起来像空格但是 non-breaking）、
zero-width spaces (U+200B/200C/200D/FEFF)、em/en dash (U+2013/2014)。
这些字符污染 markdown 文本 — owner 后续 grep / diff / 复制粘贴时
出现奇怪不匹配。

本 iter 加 ⌘⇧V "paste as plain text" handler — 读剪贴板原文 +
normalize 这几类污染字符 + 插入。覆盖 split + edit 两 textarea。

## Changes

### `src/components/panel/PanelTasks.tsx`

#### 1. `handleDetailPastePlainText` async callback（~line 3374）

```ts
const handleDetailPastePlainText = useCallback(
  async (e): Promise<boolean> => {
    if (!(e.metaKey || e.ctrlKey)) return false;
    if (!e.shiftKey || e.altKey) return false;
    if (e.key.toLowerCase() !== "v") return false;
    if (isComposing) return false;
    e.preventDefault();  // sync — 阻 native paste
    const text = await navigator.clipboard.readText();
    const clean = text
      .replace(/[“”]/g, '"')      // smart double → ASCII
      .replace(/[‘’]/g, "'")      // smart single → ASCII
      .replace(/ /g, " ")    // NBSP → space
      .replace(/[​-‍﻿]/g, "") // zero-width 系列 → 删
      .replace(/[–—]/g, "-"); // em / en dash → ASCII -
    // insert at cursor / replace selection
    setEditingDetailContent(...);
    // rAF refocus + set cursor end
  },
  [],
);
```

设计要点：
- **preventDefault sync 调**：在 await 之前，所以 native paste 已被
  阻止，无 race window。
- **clipboard.readText() 失败 silent abort**：剪贴板权限失败 / Tauri
  webview 限制时不阻塞 owner 输入流 — 静默放弃，owner 可 ⌘V 走 native。
- **normalize 规则保守**：仅 unicode → ASCII 等价映射，不破坏中文标
  点（「」『』（）等）/ emoji / 有意义的 unicode。
- **em/en dash 映射到单 `-`** 而非 `--`：保单 dash 不触 markdown
  分隔符或 list marker — owner 想要 `--` 自己敲。

#### 2. 接两 textarea onKeyDown chain（split + edit modes）

```ts
void handleDetailPastePlainText(e);
```

`void` fire-and-forget — async 函数已 sync preventDefault，async 部分
处理剪贴板 + state 更新。chain 继续后续 handler 但因 modifier 已 match
不到（⌘⇧V key=v），无 false positive。

#### 3. placeholder + cheatsheet

- 两 textarea placeholder 加 "⌘⇧V 粘贴为纯文本"
- 速查 modal 增条目 `["⌘⇧V", "粘贴为纯文本（normalize smart quotes
  / NBSP / 零宽字符 / em dash...）"]`

## Key design decisions

- **textarea 本身 paste 已是 plain，为何还要 ⌘⇧V？** textarea
  native paste 把 rich text 转成 plain 文本，但保留 unicode 字符级
  特征 — smart quotes 等"看起来像 ASCII"的字符依然原样进入。本
  handler 多走一层 normalize 把它们映射到 ASCII 等价。
- **不映射中文标点 / emoji**：那些是有意义的 unicode，owner 故意
  输入；映射会改坏文本。
- **保留 em dash 而非 `--`**：单 dash 在 markdown 不触语法（vs
  `--` 在某些 flavor 可能被解析）；owner 想要 emphasis 用 markdown
  inline `*…*` / `**…**`。
- **async + sync preventDefault 拼接**：函数顶 sync 调 preventDefault，
  await 剪贴板，最后 setState。preventDefault 优先级高于 await，
  native paste 一定被阻止。
- **chain 用 void 而非 if-return**：async 返 Promise，无法同步判断
  match；用 void fire-and-forget + 依赖后续 handler 的 modifier
  check 互斥保证不重复处理。
- **不为单 fn 引 unit test runner**：行为是 IO + state ops；build
  pass + 手测足够（从 Notion 复制 + 粘贴验 smart quotes 替换）。

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.25s)
- 后端无改动
