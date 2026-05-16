# detail.md textarea Enter 自动续列表前缀

## 背景

TODO 上 auto-proposed 一条："detail.md textarea 按 Enter 自动续列表前缀：当前行是 `- ` / `- [ ] ` / `* ` / `<N>. ` / `> ` 时新行自动复用前缀；空 list item 行 Enter 退出 list（剥前缀）。"

owner 在 detail.md 写列表 / 待办时反复手敲 `- ` / `- [ ] ` / `1. ` 等前缀很烦。IDE / Obsidian / Notion 都有"Enter 自动续列表"模式，按惯例：
1. 当前行有 list marker + 非空内容 → 新行用同 marker（有序列表 N+1）
2. 当前行仅 marker 无内容 → Enter 退出 list（剥 marker），让 owner 自然结束列表

本 iter 在 detail.md textarea onKeyDown 加 `handleDetailListContinue` handler，覆盖 4 类 marker：GFM checklist / 无序列表 / 有序列表 / blockquote。

## 改动

### `src/components/panel/PanelTasks.tsx`

#### `handleDetailListContinue` useCallback

紧贴既有 `handleDetailBracketPair` 之后。返回 `boolean` —— true 表示已处理（caller 早 return 跳过后续 ⌘S / Esc 等分支）。

```ts
const handleDetailListContinue = useCallback(
  (e: React.KeyboardEvent<HTMLTextAreaElement>): boolean => {
    if (e.key !== "Enter") return false;
    if (e.shiftKey || e.metaKey || e.ctrlKey || e.altKey) return false;
    if ((e.nativeEvent as KeyboardEvent).isComposing) return false;
    const ta = e.currentTarget;
    const start = ta.selectionStart ?? 0;
    const end = ta.selectionEnd ?? start;
    if (start !== end) return false;  // 选区非空 → native Enter 走

    const value = ta.value;
    const lineStart = value.lastIndexOf("\n", start - 1) + 1;
    const lineEnd = value.indexOf("\n", start);
    const cur = value.slice(lineStart, lineEnd === -1 ? value.length : lineEnd);

    // GFM checklist 优先（`- [ ]` 也匹配 `- ` 后续 regex，必须先排）
    let m = cur.match(/^(\s*)(- \[[ xX]\] )(.*)$/);
    if (m) {
      // empty → escape list; non-empty → 续 `- [ ] ` (default unchecked)
      // ... preventDefault + setEditingDetailContent + rAF cursor reset
      return true;
    }
    // ordered list
    m = cur.match(/^(\s*)(\d+)(\. )(.*)$/);
    if (m) {
      // empty → escape; non-empty → 续 `${N+1}. `
      // ...
      return true;
    }
    // unordered list + blockquote
    m = cur.match(/^(\s*)([-*+] |> )(.*)$/);
    if (m) {
      // empty → escape; non-empty → 续 same marker
      // ...
      return true;
    }
    return false;
  },
  [],
);
```

#### 两 textarea onKeyDown 接入

split + preview 模式两 textarea 的 onKeyDown 首行（在 bracket-pair handler 之前）加：

```ts
if (handleDetailListContinue(e)) return;
```

## 关键设计

- **优先级在 bracket-pair 之前**：bracket pair 触发字符是 `「『（【《""'`，与 Enter 互不冲突。但 list continuation 放最前是防御未来某 modifier+Enter 引入语义时的歧义；当前 list continuation 严格要求"无任何 modifier + Enter"，不抢 Shift+Enter / ⌘Enter 等。
- **三种 marker 顺序**：GFM checklist `- [ ]` → ordered `<N>.` → unordered `- ` / `* ` / `+ ` / blockquote `> `。checklist 必须最先匹配 —— `- [ ] foo` 也满足后续 `- ` 简单 regex，错配会让 checklist 退化成普通 list item。
- **escape on empty**：空 marker 行 Enter 删 marker 而非续行 —— 与 VSCode / Obsidian / Notion 同语义。让 owner "我列表写完了" 用 Enter Enter 自然退出，而非手动删 `- ` 前缀。
- **缩进保留**：每个 marker 的 indent 段 `(\s*)` 单独捕获并保留，让嵌套列表正确续行 + 嵌套层级一致。
- **有序列表自动递增**：`<N>. text` 后续是 `<N+1>. `。markdown renderer 不严格按数字渲染（实际行为按出现顺序），但 owner 期望视觉连续 `1. 2. 3.`，自动递增让 source 也整齐。
- **GFM checklist 新条 default unchecked**：续行总是 `- [ ] ` 不是 `- [x] `。owner 刚做完一件事按 Enter 想写下一件 —— 下一件还没做，应该 `[ ]`。
- **选区非空跳过**：用户选了 N 字符按 Enter（标准行为是替换选区为 `\n`）—— 让 native 走。
- **IME composing 跳过**：与 bracket pair 同 guard，避免输入法候选 Enter 被抢。
- **rAF + setDetailCursorPos 同步**：与 bracket pair / insertDoneLineAtCursor 等同模式，让底部状态栏行号 chip 即时跟上 cursor 新位置。

## 不做

- **不接 markdown 表格行续**：表格语法 `| ... |` 是块级，按 Enter 续行更复杂（要数列数 / 添 `| | | |` 模板）。当前 ✓ table 工具栏按钮已 cover 创建场景；后续表格行续可单独 iter。
- **不接代码块内**：` ``` ` fence code 内按 Enter 不应续 list marker（用户写代码 / shell 命令）。检测需要扫光标位置是否在 fence 内，复杂度 +1。当前忽略 —— code fence 内 owner 通常不会先写 `- ` 然后期待 list continuation。
- **不写测试**：纯字符串 split + regex；既有 bracket pair / insertDoneLineAtCursor / line-prefix 模式都视觉验证。手动测：textarea 输入 "- foo<Enter>" → 看到 "- foo\n- " + cursor 在末尾。
- **不接桌面 ChatMini / PanelChat textarea**：chat 输入框场景下用户大概率不期待 list continuation（输 `- 选项A` 可能是临时讨论，不想自动续）。本 iter 专注 detail.md 长文写作。
- **不持久化偏好开关**：list continuation 是常见 IDE 行为，全开默认。极少用户会希望禁用 —— 真要禁用走 Shift+Enter 单次绕开。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.17s
- 改动 ~150 行（handleDetailListContinue useCallback 140 + 2 个 textarea 1 行接入 + 注释）；既有 handleDetailBracketPair / ⌘S / Esc / setEditingDetailContent / setDetailCursorPos 路径完全不动。

## TODO 状态

6 条 auto-proposed 已完成 1 条，余 5 条留池：
- 桌面 pet 主区顶部 "今天 N 条 · 主动 M 次" ambient
- PanelSettings 🔌 测试 LLM 连通性按钮
- detail.md textarea ⌘D 复制当前行
- proactive prompt 加最近 24h 完成任务
- PanelChat sort chip

## 后续

- code fence 内禁用 continuation（扫光标是否在 ` ``` ` 内）。
- ⌘\\ 关闭 continuation（让 owner 一次跳出 IDE 行为）—— 边际收益不大。
- 自动重写 ordered list 全局重编号：当 owner 删除中间某行后，下方剩余项 `2. 3. 4.` 自动 shift 成 `2. 3.`。属"smart numbering"高阶 IDE 行为，比简单续行复杂得多。
