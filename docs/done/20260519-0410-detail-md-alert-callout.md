# detail.md 编辑器加「⌘⇧A 插 GFM alert callout」快捷键（iter #530）

## Background

owner 写决策日志 / 进度笔记常想强调段（提示 / 警告 / 风险 / 注意）。
GFM 4 种 markdown alert：

- `> [!NOTE]` — 一般提示（蓝 callout）
- `> [!TIP]` — 建议 / 最佳实践（绿）
- `> [!WARNING]` — 警告（黄）
- `> [!CAUTION]` — 严重风险（红）

但手敲 `> [!NOTE]\n> ` syntax 容易：
1. 漏空行（前后必空行才被 GFM 识别为 callout，否则当普通 blockquote）
2. 大小写错（`[!note]` 不行）
3. 忘了第二行也要 `> ` 前缀

本 iter 加 ⌘⇧A — 直接插「`> [!NOTE]\n> `」模板，cursor 落第二行 `> `
之后准备打内容。

## Changes

### `src/components/panel/PanelTasks.tsx`

新 `handleDetailAlertTemplate` callback（紧贴 `handleDetailTableTemplate`
之后）：

```tsx
const handleDetailAlertTemplate = useCallback(
  (e: React.KeyboardEvent<HTMLTextAreaElement>): boolean => {
    if (!(e.metaKey || e.ctrlKey)) return false;
    if (!e.shiftKey || e.altKey) return false;
    if (e.key.toLowerCase() !== "a") return false;
    if ((e.nativeEvent as KeyboardEvent).isComposing) return false;
    e.preventDefault();
    // 默认 [!NOTE]；其它 3 种 owner 手动改 NOTE → TIP/WARNING/CAUTION
    const template = "\n> [!NOTE]\n> ";
    insertMarkdownAtCursor("wrap", template, "");
    return true;
  },
  [insertMarkdownAtCursor],
);
```

#### 接入 onKeyDown 链

两个 textarea（split / edit-only）都接：

```tsx
if (handleDetailTableTemplate(e)) return;
// ⌘⇧A 插 GFM markdown alert callout 模板（`> [!NOTE]` + cursor 落第二行）
if (handleDetailAlertTemplate(e)) return;
```

#### Keyboard help modal 新一行

```tsx
["⌘⇧A", "插 GFM markdown alert callout（默认 [!NOTE]；手改 TIP/WARNING/CAUTION）"],
```

## Key design decisions

- **默认 [!NOTE] 单 shortcut**：与其占 4 个键位（⌘⇧A/T/W/C 各对应一
  种）— owner 写 NOTE 最多，TIP/WARNING/CAUTION 手敲改 5 个字符 cost
  极低。保持键位精简
- **前后空行包裹**：GFM spec — callout 必须前后空行才被识别为
  alert block，否则当普通 `> ` blockquote。`\n> [!NOTE]\n> ` 前面 `\n`
  保前空行；后面 `> ` 行末无 `\n` 让 cursor 自然落此（owner 即时打
  内容）
- **modifier ⌘⇧A**：⌘A 是 select all；shift 修饰避开。⌘⇧A 在 IDE 间
  mostly 空（VS Code "toggle sticky scroll" 等冷门绑定）— 占给 "Alert"
  助记
- **cursor 落第二行 `> ` 后**：复用 `insertMarkdownAtCursor("wrap", ...)`
  insert + 后置 cursor 行为（既有 ⌘⇧M table template 同 helper）— 让
  owner 立即敲 callout body 不必再光标定位
- **不写 unit test**：纯字符串模板 + 既有 `insertMarkdownAtCursor`
  helper（production 验证）。逻辑 trivial — GOAL.md "meaningful tests
  only" 规则下不引装饰性测试
- **不引「选 alert 类型 popover」**：owner 想要 WARNING/CAUTION 时手
  改 4 个字母 cost < popover open/close + select friction；模板派
  shortcut 设计就是「一键 default，需要变种手改」

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.35s)
- 后端无改动 — 纯前端 keyboard shortcut
- 手测：
  - detail.md 编辑器内 ⌘⇧A → 插「> [!NOTE]\n> 」模板 + cursor 落第二
    行 `> ` 后
  - 输内容 → preview 看到 NOTE callout 蓝框渲染
  - 手改 NOTE → WARNING → 看 preview 切到黄框
  - 跨 split / edit-only 模式都触发
  - ⌘/ 帮助 modal 看到新「⌘⇧A」行

## Future iters (out of scope)

- 「⌘⇧⌥A cycle alert type」— 按一次切下一种（NOTE → TIP → WARNING →
  CAUTION → NOTE）；当前手改足够
- 「⌘⇧⌥W 直接插 WARNING / ⌘⇧⌥C 直接插 CAUTION」— 高频时再 propose
- 「按选区 wrap as callout」— 选段已有内容时 wrap 成 callout（非空选
  区路径）；当前 wrap 是 insert-at-cursor，未来 iter 可扩
