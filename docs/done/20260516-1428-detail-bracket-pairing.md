# 任务 detail.md textarea 中文配对引号 / 括号

## 背景

TODO 上 auto-proposed 一条："任务 detail.md textarea 中文配对引号 / 括号：输 `「` 自动补 `」`，与 IDE bracket 配对同模式。"

owner 在 detail.md 写中文笔记时常用 `「」` / `（）` / `【】` / `《》` 等成对 typography 符号引用 / 强调。手敲 open 后还得敲 close + 把光标退回中间，多 2-3 步操作。IDE-style auto-pair 让 owner 输 `「` 即得 `「|」`（光标在中间）一步到位 —— VSCode / Sublime / Obsidian 都这么干。

## 改动

### `src/components/panel/PanelTasks.tsx`

#### 配对表

模块作用域：

```ts
const BRACKET_PAIRS: Record<string, string> = {
  "「": "」",
  "『": "』",
  "（": "）",
  "【": "】",
  "《": "》",
  "“": "”",
  "‘": "’",
};
```

只含 Chinese typography / 全角字符。ASCII `(` / `[` / `{` 故意不加 —— 那些容易误触（owner 写代码 / 数学表达式 / 命令时不期待自动配对）。

#### handler

`handleDetailBracketPair(e)` useCallback 在组件作用域：

```ts
const handleDetailBracketPair = useCallback(
  (e: React.KeyboardEvent<HTMLTextAreaElement>): boolean => {
    const close = BRACKET_PAIRS[e.key];
    if (!close) return false;
    // IME composing：让输入法自处理（搜狗 / Mac default 等候选浮窗期间不抢键）。
    if ((e.nativeEvent as KeyboardEvent).isComposing) return false;
    const ta = e.currentTarget;
    const start = ta.selectionStart ?? 0;
    const end = ta.selectionEnd ?? start;
    e.preventDefault();
    const value = ta.value;
    const selected = value.slice(start, end);
    const inserted = e.key + selected + close;
    const next = value.slice(0, start) + inserted + value.slice(end);
    setEditingDetailContent(next);
    const innerStart = start + e.key.length;
    const innerEnd = innerStart + selected.length;
    setDetailCursorPos(innerStart);
    requestAnimationFrame(() => {
      const cur = detailEditorRef.current;
      if (!cur) return;
      cur.focus();
      cur.selectionStart = innerStart;
      cur.selectionEnd = innerEnd;
    });
    return true;
  },
  [],
);
```

#### 两个 textarea 接入

edit 模式 + split 模式两个 textarea onKeyDown 首行都加：

```ts
if (handleDetailBracketPair(e)) return;
```

放在 ⌘S / Esc 分支**之前** —— 字符级 intercept 不该被 modifier key 路径抢走。

## 关键设计

- **空选区 / 非空选区两态**：
  - 空：插入 open + close，光标落 inner（cursor 在 pair 中间）—— VSCode 模式
  - 非空：包裹选区为 open + selection + close，selection 仍是 inner content —— 让 owner 可继续 typing 或嵌套包裹（输 `「`包文，再输 `「` 包成 `「「文」」`）。
- **IME composing 跳过**：搜狗 / 微软 / Mac default IME 候选浮窗期间，键盘事件含 `isComposing: true` —— 让输入法自己处理 keystroke，我们不抢。`React.KeyboardEvent.nativeEvent.isComposing` 是访问 native flag 的标准方式（React 不直接暴露）。
- **只 7 个 typography 字符**：刻意不含 ASCII bracket / quote。ASCII `'` 是代码缩写 / `("hello")` 是函数调用 / `[1,2,3]` 是数组 —— 这些场景 owner 不期待自动配对，加进来反成噪音。中文 typography 字符使用场景纯粹（引用 / 引号 / 强调），auto-pair 准确率高。
- **空选区时插入位置 = innerStart === innerEnd**：cursor 单点而非 selection；user 立刻可 typing。
- **非空选区时保 inner selection**：让 user 看到"已包裹"且可继续操作。VSCode 同模式。
- **`setDetailCursorPos` 同步**：底部状态栏「行 N / 共 M」chip 即时跟上 cursor 位置。与 insertCurrentTimeAtCursor / insertDoneLineAtCursor 同 pattern。
- **rAF 设 selection 不是同步 set**：setEditingDetailContent 是异步 batch；rAF 让 React 重渲 textarea 再设 selectionRange 才能正确。

## 不做

- **不支持 ASCII bracket 自动配对**：见上"只 7 个 typography 字符"理由。需要时 owner 显式用 markdown 工具栏 **🔗** 按钮（已有 `[text](url)` 包裹）。
- **不做 over-type close**：IDE 常见行为是"光标在 close 字符前再按 close → 跳过插入移动光标"。当前简化不做，user 多敲一个 close 自己删 —— 边际收益 vs 实现复杂度不值。
- **不做 smart Backspace**：IDE 行为是"刚自动配对后按 Backspace → 删两边"。本 iter 不做，user 多按一次 Backspace 完成 —— 渐进。
- **不在 PanelChat textarea 同样接入**：chat 输入框 IME 比 detail 编辑更高频（每条消息都用），自动配对干扰风险更大；且 owner 在 chat 里多打长引号场景少。本 iter 专注 detail.md 长文写作场景。
- **不写测试**：纯 keydown + state mutation；既有 insertDoneLineAtCursor / insertCurrentTimeAtCursor / insertMarkdownAtCursor 等同类无单测。视觉验证（输 `「` → 看到 `「|」` cursor 中间）足够。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.15s
- 改动 ~80 行（BRACKET_PAIRS const 12 + handleDetailBracketPair callback 40 + 2 个 textarea 1 行接入 + 注释）；既有 onKeyDown 的 ⌘S / Esc 分支 / setEditingDetailContent / setDetailCursorPos 路径完全不动。

## TODO 状态

6 条 auto-proposed 已完成 4 条，余 2 条留池：
- PanelMemory 类目内 items > 20 时按 updated_at 月份分组
- detail.md preview「📑 大纲」浮窗

## 后续

- smart Backspace：刚自动配对后立刻 Backspace 删两边，与 VSCode 一致。
- over-type close：光标在 close 字符前按 close → 跳过插入，仅移动光标。
- markdown bracket（`[` / `(` / `{`）作为可选配对：通过 settings 暴露开关，让 owner 按个人偏好启用 / 禁用 ASCII pair。
- 加 `**` / `_` / `` ` `` 等 markdown 包装字符（成对 inline 格式）的同款配对 —— 与 markdown 工具栏的 wrap 模式有交集；要 dedupe 设计。
