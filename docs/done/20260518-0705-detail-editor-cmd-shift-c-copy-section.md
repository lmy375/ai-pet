# detail.md 编辑器「⌘⇧C 复制当前段」shortcut（iter #458）

## Background

detail.md preview 已有「📋 复制此节」按钮（每个 heading 旁的 inline
chip — 复用 `extractSectionFromMarkdown` + `handleCopyHeadingSection`）。
但按钮入口要 mouse 点 + scroll 到目标 heading；键盘党在编辑 textarea
里想"复制我正在写的这段"时仍要切到 preview / split mode 手点。

本 iter 加 ⌘⇧C — 光标在哪段就复制哪段 markdown heading 段（heading 行
到下一同级或更高级 heading 之间）。与既有 ⌘B / ⌘I / ⌘U / ⌘⇧D 等
detail.md shortcut 家族共生。

## Changes

### `src/components/panel/PanelTasks.tsx`

#### 1. `extractSectionAroundCursor` 新 helper

紧贴 `extractSectionFromMarkdown` 之前：

```ts
function extractSectionAroundCursor(md: string, cursor: number): string {
  const before = md.slice(0, Math.max(0, Math.min(cursor, md.length)));
  const cursorLine = before.split("\n").length - 1;
  const lines = md.split("\n");
  // 向上找最近的 heading
  let startIdx = -1, startLevel = 0;
  for (let i = cursorLine; i >= 0; i--) {
    const m = lines[i].match(/^(#{1,3})\s+/);
    if (m) { startIdx = i; startLevel = m[1].length; break; }
  }
  if (startIdx < 0) {
    // 光标在第一个 heading 前 — 返 preamble
    let firstHeading = lines.length;
    for (let i = 0; i < lines.length; i++) {
      if (/^#{1,3}\s+/.test(lines[i])) { firstHeading = i; break; }
    }
    return lines.slice(0, firstHeading).join("\n").trimEnd();
  }
  // 向下找下一同级或更高级 heading（`<= startLevel`，与 extractSectionFromMarkdown
  // 同 boundary 协议）
  let endIdx = lines.length;
  for (let i = startIdx + 1; i < lines.length; i++) {
    const m = lines[i].match(/^(#{1,3})\s+/);
    if (m && m[1].length <= startLevel) { endIdx = i; break; }
  }
  return lines.slice(startIdx, endIdx).join("\n").trimEnd();
}
```

设计：
- **入参 cursor 偏移而非 heading counter**：与 button-input
  extractSectionFromMarkdown 互补 — 键盘场景用 cursor，mouse 场景用
  heading-counter
- **preamble fallback**：光标在首 heading 之前 → 返文首到首 heading 之
  间内容。让 owner 复制 detail.md 顶部的"摘要 / context"段不必先加 heading
- **`<= startLevel` boundary**：H1 段含子 H2/H3；H2 段含 H3；H3 段仅
  自身。与既有 extractSectionFromMarkdown 同协议保一致
- **`trimEnd()`**：去尾巴空行让粘出去干净

#### 2. `handleDetailCopySection` keyboard handler

紧贴既有 `handleDetailDateStamp`（⌘⇧D 同 modifier 家族）之前：

```ts
const handleDetailCopySection = useCallback((e) => {
  if (!(e.metaKey || e.ctrlKey)) return false;
  if (!e.shiftKey || e.altKey) return false;
  if (e.key.toLowerCase() !== "c") return false;
  if ((e.nativeEvent as KeyboardEvent).isComposing) return false;
  e.preventDefault();
  const ta = e.currentTarget;
  const cursor = ta.selectionStart ?? 0;
  const section = extractSectionAroundCursor(ta.value, cursor);
  if (!section) { ...miss feedback... return true; }
  const head = section.split("\n")[0].slice(0, 30);
  navigator.clipboard.writeText(section).then(() => {
    setBulkResultMsg(`📋 已复制当前段（${section.length} 字 · ${head}）`);
    ...
  }).catch(...);
  return true;
}, []);
```

#### 3. 接入两 textarea onKeyDown 链

split 模式 + 纯 edit 模式两条 chain 都加 `if (handleDetailCopySection(e)) return;`
紧贴 `handleDetailDateStamp` 之后。

## Key design decisions

- **⌘⇧C 选键**：⌘C 是 textarea 选区 → clipboard 默认行为（不抢）；
  ⌘⇧C 在 Chrome devtools 是「inspect element」但 Tauri webview 已禁，
  preventDefault 兜底安全。⌘⇧C 与 "Section / Copy" 语义自然映射
- **复用 `extractSectionAroundCursor` 而非直接调
  extractSectionFromMarkdown(counter)**：cursor offset 走 textarea
  selectionStart 自然；要算 heading counter 还得正向数到当前位置 —
  两层包装。直接以 cursor 为锚点最简
- **preamble fallback 输出 vs 直接 "无段" 失败兜底**：detail.md 顶部
  常有 owner 写的 "context 段"（如简介 / TL;DR），不一定有 heading。
  让该段也能 ⌘⇧C 复制是 owner 心智符合（"我光标在这段，按 ⌘⇧C 应
  复制这段"）
- **section.length 字数 + heading head 30 字 preview 在 toast**：让
  owner 即时验证「我复制的是哪段」— heading text 高速可识别（比"已
  复制"通用文案有用得多）
- **fail 路径返 true 而非 false**：handler 已 preventDefault，链上
  下游不该继续；返 true 标"已处理"
- **不写 unit test**：纯 string-extract + clipboard 副作用；
  `extractSectionAroundCursor` 与既有 `extractSectionFromMarkdown` 同
  算法 family，后者 production 验证；`tsc` + `vite build` clean 即够。
  GOAL.md "meaningful tests only" 规则下不引装饰性测试
- **接入两 chain 显式列**：split + edit 两 textarea 独立 onKeyDown
  chain（既有架构）— 与 ⌘⇧D 同样在两处显式接入保一致性

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.27s)
- 后端无改动 — 纯前端 keyboard handler + 一个 helper
- 手测：detail.md 编辑 → 光标停在 H2 段中间 → ⌘⇧C → 看 toast「📋 已
  复制当前段（N 字 · ## heading-name…）」→ 粘到 markdown 编辑器看
  H2 + 子 H3 段完整复制；光标在文首 preamble → ⌘⇧C → 复制 preamble
