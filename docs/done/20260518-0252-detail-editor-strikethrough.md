# detail.md 编辑器「⌘U 删除线」shortcut（iter #446）

## Background

detail.md 编辑器已有 ⌘B (`**bold**`) / ⌘I (`*italic*`) / ⌘\`
(fenced code block) wrap-mode shortcuts。markdown GFM 删除线 `~~text~~`
是常用语义（如"~~原计划~~ 改成 …"／"~~取消~~ 后又恢复"），但 owner
要么手工敲 `~~` 要么放弃使用 — 没快捷键入口。

本 iter 补 ⌘U → 选区 wrap `~~...~~` markdown 删除线，与 ⌘B / ⌘I
同 wrap-mode 模板。

## Changes

### `src/components/panel/PanelTasks.tsx`

#### 1. `handleDetailBoldItalic` 内加 `u` case

```ts
if (key === "u") {
  e.preventDefault();
  insertMarkdownAtCursor("wrap", "~~", "~~");
  return true;
}
```

复用既有 wrap-mode 算法 — 选区有内容 → wrap；无选区 → 插模板 + 光标
落 `~~ | ~~` 中间。同 modifier check（meta/ctrl only，无 shift / alt）+
IME composing skip。

注：浏览器默认 ⌘U 是"查看页面源代码" — Tauri webview 已禁但
preventDefault 仍是保险（debug 构建偶有泄漏）。

#### 2. 工具栏 S 按钮

紧贴 B 按钮之后、• 列表按钮之前：

```tsx
<button onClick={() => insertMarkdownAtCursor("wrap", "~~", "~~")}
        title="删除线（~~...~~）。GFM markdown 渲为划线文字。…⌘U 同效。"
        style={mdToolbarBtnStyle}>
  <s>S</s>
</button>
```

`<s>S</s>` 让 button 自身就显划线 S — visual 与功能匹配，与 `<strong>B</strong>`
（加粗 B）同 inline-style 协议。

#### 3. B 按钮 tooltip 补 ⌘B 同效

既有 B tooltip 没提快捷键，本 iter 顺手补「⌘B 同效」让 owner 在 hover
看 tooltip 时发现快捷键。

## Key design decisions

- **⌘U 选 underline 标准位**：常见 IDE / 富文本编辑器 ⌘U 默认是
  underline，但 markdown 没原生 underline 语义。GFM 删除线 `~~`
  是「删 / 否定」的 markdown 标准等价 — 用 ⌘U 抢这个 slot 让 owner
  从 Word / Google Docs 等"⌘U = 强调删 / 划"心智迁移过来零成本
- **wrap-mode `~~ ~~`（双侧对称）**：GFM 协议 — 单侧 `~` 是删除线，
  双侧 `~~` 是定式（与 underline 协议混在一起的废弃方言不取）。两个
  `~` 让 owner 删除线视觉更明显，也减少与 `~/path/to/file` 路径意外
  撞车的可能
- **不引 ⌘\ ⌘\\ 等其它键当 alias**：UI shortcut 集群已大；保 ⌘B/I/U
  三键一族即可。⌘⇧X 等 Word 风格删除线 alias 学习曲线更高
- **modifier 严格（无 shift / alt）**：与 ⌘B / ⌘I 同；⌘⇧U / ⌥⌘U
  留作未来扩展不抢
- **工具栏 button 也加**：让 mouse 党也能用。`<s>S</s>` 让 button 一
  眼显「划线 S」与文案对齐；与 `<strong>B</strong>` 同 inline-style
  协议（不依赖 CSS class）
- **不写 unit test**：纯 keyboard handler 调既有 `insertMarkdownAtCursor`
  helper（其行为已 production 验证）。GOAL.md "meaningful tests only"
  规则下，本变更只是 dispatch 表加一条 — 装饰性测试不引入

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.25s)
- 后端无改动 — 纯前端 keyboard handler + 1 toolbar button
- 手测：detail.md 编辑模式 → 选中文字 ⌘U → 看 wrap 成 `~~text~~` →
  preview 渲为划线 → 工具栏 S 按钮 click 同效
