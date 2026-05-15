# detail.md 工具栏「✓ 完成行」按钮

## 背景

TODO 上 auto-proposed 一条："detail.md 工具栏「✓ 完成行」按钮：插 `- [x] YYYY-MM-DD HH:MM ` 行首模板，记『我刚刚做完了什么』零摩擦。"

上一轮的 📅 时间戳按钮已让"插当前时间"成 1 步。但实际 detail.md 用例里更高频的是"做完一小步 + 记下来什么时候做的"—— 这是 owner / 宠物轮流写流水账的标准格式。手敲 `- [x] 2026-05-14 18:22 ` 13 个字符 + 多种符号边界（括号 / 数字 / 空格），高摩擦。

把"checklist done marker + 时间戳 + 光标落尾"做成单按钮，让 quick log 落到肌肉记忆：点 → 敲摘要 → Enter（换行）/ 继续。

## 改动

### `src/components/panel/PanelTasks.tsx`

#### helper

```ts
const insertDoneLineAtCursor = useCallback(() => {
  const ta = detailEditorRef.current;
  if (!ta) return;
  const start = ta.selectionStart ?? 0;
  const value = ta.value;
  // 光标当前行的行首位置：从 start 往前找最近的 `\n`，行首 = idx + 1。
  const lineStart = value.lastIndexOf("\n", start - 1) + 1;
  const now = new Date();
  const stamp = `- [x] ${YYYY}-${MM}-${DD} ${HH}:${MM} `;
  const rest = value.slice(lineStart);
  if (/^\s*- \[[ xX]\] /.test(rest)) {
    // 已是 checklist 行 → 跳过 + toast 提示原因
    setActionErr("当前行已是 checklist；想改时间戳请删后重插或手动编辑。");
    setTimeout(() => setActionErr(""), 3500);
    return;
  }
  const next = value.slice(0, lineStart) + stamp + value.slice(lineStart);
  const cursorPos = lineStart + stamp.length;
  setEditingDetailContent(next);
  requestAnimationFrame(() => {
    cur.focus();
    cur.selectionStart = cur.selectionEnd = cursorPos;
  });
}, []);
```

#### 按钮

工具栏末尾、紧贴 📅 时间按钮之后：

```tsx
<button onClick={insertDoneLineAtCursor} title="✓ 完成行（- [x] YYYY-MM-DD HH:MM ）...">✓</button>
```

## 关键设计

- **行首插入而非光标位置插入**：用户在长 detail.md 任意位置点按钮 → 当前行首加完成 marker，比强制在光标精确位置插入更直觉（quick log 是行级语义）。`lineStart = value.lastIndexOf("\n", start - 1) + 1` 是与既有 `line-prefix` mode 同一种行首寻址。
- **光标落到 stamp 末尾**：用户按完按钮立即可以敲"做了什么"，与既有 📅 按钮的"光标落尾"行为一致 —— 输入流不会被打断。
- **当前行已是 checklist 时跳过 + toast 解释**：连点会让模板叠加成 `- [x] xxx - [x] yyy` 这种损坏的 markdown；用 regex 检查 + 给原因 toast 比静默 noop 友好。提示用户用既有 ☐ 按钮或手动改。
- **复用既有 GFM checklist 渲染**：detail.md 渲染层早已识别 `- [x]` 为已勾选 checkbox（disabled）+ line-through 视觉。新按钮 0 改 markdown 渲染层，纯输入工具。
- **`- [x]` 而非 `- [ ]`**：`✓ 完成行` 语义即"做完了"。要"加未完待办"还有既有 ☐ 按钮（line-prefix `- [ ]`）。两个按钮各自承担"刚做完" vs "等会儿做"语义。
- **`✓` 单字符 icon**：与既有 `B` / `•` / `🔗` / `</>`/ `☐` / `❝` / `📊` / `📅` 等 emoji + symbol mix 风格一致；`✓` 比 `✅` emoji 朴素，符合 "checklist 已勾选" 的内敛感。

## 不做

- **不写键盘快捷键**：toolbar 9 个按钮都没快捷键；单独给"完成行"加 `⌘D` / `⌘Enter` 等可能与未来扩展冲突，留待整体快捷键梳理时再补。
- **不接 detail.md 之外**：本入口只在任务 detail.md 编辑器（与既有 8 个 toolbar 同位）。memory / chat / settings 等编辑器各自有不同用例，不污染。
- **不写测试**：纯 DOM textarea + Date + regex 操作，逻辑 30 行；vitest jsdom 下 selection API / requestAnimationFrame / 时钟桩接复杂度远大于价值。视觉验证即可。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.23s
- 改动 ~50 行（helper 35 + 按钮 7 + comment）；既有 8 个 toolbar 按钮 / textarea 路径不变。

## TODO 状态

5 条候选 auto-proposed 已完成 1 条，余 4 条留池：
- 任务标题双击 inline 重命名
- ✨ LLM 重写任务标题
- mini chat 顶部上下文 token 提示 chip
- PanelChat 顶部「📌 钉住会话」 chip 计数
- detail.md textarea 底部行号 status bar

（下一轮直接接 iter2 / iter3 实现。）

## 后续

- "✓ 完成行" + "📅" 触发后自动加 newline 起新行：让 quick log 可"连按"。但默认行为复杂度增加，先观察用户实际使用。
- toolbar 按钮密度太大（9 个）时分组折叠：常用 5 个常驻 + "..." 弹更多。等再加 11 个左右再做。
