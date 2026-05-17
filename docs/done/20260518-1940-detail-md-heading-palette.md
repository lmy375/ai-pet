# detail.md 编辑器加「⌘⇧P heading palette」shortcut（iter #493）

## Background

detail.md 编辑器 owner 在长文档（≥ 数千字 / ≥ 数十 heading）里写笔记
时，跨段跳转只能靠 ⌘F 行内搜索 + 手动滚动。VS Code / Obsidian / Typora
都有 "Outline / Symbol palette" 入口 — fuzzy 输 heading 文本即可跳到
那段。

本 iter 加 detail.md textarea 焦点内的 **⌘⇧P** 快捷键 — 弹 fuzzy
palette 列文档所有 markdown headings，Enter 跳到 heading 行首。

## Modifier choice

TODO 原 wording 是「⌘P」但 **⌘P 已被占** — line 2764 是 "切到
preview-only 模式（VSCode preview-lock 风）"。已是文档化 + 在 ⌘/ 帮助
modal 列出的固定 binding，破坏会扰用户肌肉记忆。

故改用 **⌘⇧P** — VS Code "Command Palette" 修饰约定，与本功能（弹
fuzzy palette）语义一致，且与既有 ⌘⇧L (link popover) / ⌘⇧K (delete
line) / ⌘⇧D (date stamp) / ⌘⇧C (copy section) / ⌘⇧V (paste plain)
同 modifier 集群一致。

## Changes

### `src/components/panel/PanelTasks.tsx`

#### State

```tsx
const [headingPaletteOpen, setHeadingPaletteOpen] = useState(false);
const [headingPaletteQuery, setHeadingPaletteQuery] = useState("");
const [headingPaletteIdx, setHeadingPaletteIdx] = useState(0);
const headingPaletteInputRef = useRef<HTMLInputElement>(null);
```

#### Heading 解析（pure useMemo）

```tsx
const headingsInContent = useMemo(() => {
  // 扫 editingDetailContent 取 ATX-style headings：
  // ^{0,3 spaces}#{1,6}\s+<text>$
  // - 仅 ATX，不识别 setext（=== / --- 下划线 — 现代 markdown 已少用）
  // - fenced code block (``` / ~~~) 内的 # 不算 heading
  // - CommonMark 允许行首 ≤ 3 空格；≥ 4 空格起算 indented code block
  ...
  return out; // { level: 1-6, text, lineStart }[]
}, [editingDetailContent]);
```

#### Filter + jump

```tsx
const filteredHeadings = useMemo(() => {
  const q = headingPaletteQuery.trim().toLowerCase();
  if (q.length === 0) return headingsInContent;
  return headingsInContent.filter((h) => h.text.toLowerCase().includes(q));
}, [headingsInContent, headingPaletteQuery]);

const jumpToHeading = useCallback((lineStart: number) => {
  const ta = detailEditorRef.current;
  if (!ta) return;
  ta.focus();
  ta.selectionStart = ta.selectionEnd = lineStart;
  // textarea native focus + setSelectionRange 自带 scrollIntoView，rAF
  // 后微调 scrollTop 留 40px 头顶缓冲
  requestAnimationFrame(() => {
    ta.scrollTop = Math.max(0, ta.scrollTop - 40);
  });
  setHeadingPaletteOpen(false);
}, []);
```

#### onKeyDown 链

```tsx
const handleDetailHeadingPalette = useCallback(
  (e: React.KeyboardEvent<HTMLTextAreaElement>): boolean => {
    if (!(e.metaKey || e.ctrlKey)) return false;
    if (!e.shiftKey || e.altKey) return false;
    if (e.key.toLowerCase() !== "p") return false;
    if ((e.nativeEvent as KeyboardEvent).isComposing) return false;
    e.preventDefault();
    e.stopPropagation();
    setHeadingPaletteOpen(true);
    setHeadingPaletteQuery("");
    setHeadingPaletteIdx(0);
    window.setTimeout(() => {
      headingPaletteInputRef.current?.focus();
      headingPaletteInputRef.current?.select();
    }, 0);
    return true;
  },
  [],
);
```

Wired into both split-mode + edit-only-mode textarea onKeyDown chains
紧跟 `handleDetailLinkPopover` 之后。

#### Popover UI

模板与既有 `linkPopoverOpen` 风一致（fixed overlay + paddingTop: 14vh +
backdrop click close）。区别：

- 480px 宽（容纳 heading 全文 + level prefix）
- `maxHeight: 70vh`、列表区 `maxHeight: 50vh` + `overflowY: auto`
- 每条 entry: `H{level}` mono prefix + 按 level 缩进的 heading text
- ↑↓ 导航 + Enter 提交 + Esc 关 + hover 高亮 + 点击直跳
- 空文档：「本文档无 markdown heading（# / ## / ...）」+ 教学提示
- 空过滤结果：「（无匹配项）」

#### Keyboard help modal

⌘/ 帮助 modal 新增一行 `⌘⇧P` 行紧贴既有 `⌘P` 行下，与 modifier 升序
排序一致。

## Key design decisions

- **⌘⇧P 而非 ⌘P**：⌘P 已占（preview-only toggle）。⌘⇧P 是 VS Code 标
  准「Command Palette」修饰，与本功能语义一致 + 与既有 ⌘⇧K/L/D/C/V
  集群同 modifier — owner 心智一致
- **ATX-only, fenced-aware**：CommonMark spec：仅 ATX（`#` 前缀）算 heading；
  setext（=== / --- 下划线）形态稀少，跳过避复杂度。fenced code block
  内的 # 是 shell / python 注释，跳过避误识别
- **useMemo derive headings**：每次 editingDetailContent 改重算 — O(n)
  扫一遍即可。filteredHeadings 二级 memo 防 input typing 重 derive 全
  量 list
- **lineStart 是字符 offset 不是行号**：textarea selectionStart 直接接
  受 char offset，无需行号 → offset 映射；与既有 cursor handling 一致
- **scroll 头顶留 40px**：textarea focus + setSelectionRange 默认会让选
  区在视口底部，体验是 "heading 紧贴视口顶"；scrollTop -40 留缓冲让
  heading 上一行也露 — 阅读上下文舒服
- **空文档教学提示**：headingsInContent.length===0 时显「用 toolbar
  H1 / H2 / # 标记添加」— 新 owner 第一次按 ⌘⇧P 想知道为啥空
- **复用 linkPopover 风**：fixed overlay + backdrop dismiss + 14vh
  top + var(--pet-shadow-md) 一致 — 既有 popover 视觉 family
- **filteredHeadings 不限上限**：长 doc 可能 100+ headings — palette
  设计就是给长 doc 用的，限 30 反而漏；overflowY 滚动负责长度
- **mouseEnter 设 idx**：让 ↑↓ 与 hover 状态一致 — owner 移鼠标 +
  Enter 也能跳，键鼠混用顺畅
- **stopPropagation on ⌘⇧P**：防 onKeyDown 链下游某 handler 抢键（虽
  无已知冲突，但既有 ⌘P toggle 已有 stopImmediatePropagation 防御，本
  handler 一致）
- **不写 unit test**：纯 React state + heading 解析（CommonMark spec
  edge case 已 fenced-aware + 3-space tolerant）+ DOM 操作。逻辑足够
  trivial（既有 ⌘⇧L link popover / ⌘K task palette 同 pattern
  production 验证）。GOAL.md "meaningful tests only" 规则下不引装饰性
  测试

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.29s)
- 后端无改动 — 纯前端 keyboard shortcut + popover
- 手测：detail.md 编辑器（split / edit-only 两 mode 都 OK）
  - 长 doc 含多 heading → ⌘⇧P → palette 弹 + 输入框 focus + 列全 heading（按文档序，H{level} prefix + indent）
  - 输关键词 → fuzzy 实时过滤 → ↑↓ 选 + Enter 跳 heading 行首
  - Esc 关 / 点 backdrop 关
  - 空 heading doc：「本文档无 markdown heading」+ 教学
  - fenced code block 内 `# comment` 不被识别为 heading（验证 fenced gate）
  - ⌘P 仍是 preview-only toggle（未被破坏）

## Future iters (out of scope)

- toolbar 加 「📑 outline」按钮：与 ⌘⇧P 同入口但点击触发 — 鼠标党友好
- heading 旁加 line number indicator（如「H2 · 行 42」）— 给 owner 视
  觉锚点
- ranking：fuzzy match 用 sublime-style char-by-char score 而非纯子串
  — 当前已够用，复杂度收益不匹配
- 跳后高亮 heading 行（短暂 yellow flash）—视觉反馈"我跳到了哪里"
