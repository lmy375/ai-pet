# detail.md 编辑器「⌘⇧D 插日期戳」shortcut（iter #452）

## Background

detail.md 编辑器既有 📅 toolbar 按钮 `insertCurrentTimeAtCursor` 插全
形 `YYYY-MM-DD HH:MM`（与 [snooze:] / [once:] marker 协议同形，适合
独立段落 ts）。但 progress note 的 inline 标注场景 — 比如 "checked
at 05-18 04:30 → still fails" / "再试一次 (05-18 11:00)" — 用全形
浪费 5 字符（year 在同年内 obvious），且视觉略重。

本 iter 加 ⌘⇧D 键盘快捷键 — 插短形 `MM-DD HH:MM`，与既有 📅 toolbar
全形互补。

## Changes

### `src/components/panel/PanelTasks.tsx`

#### 1. `handleDetailDateStamp` handler

紧贴既有 `handleDetailBoldItalic` 之后：

```ts
const handleDetailDateStamp = useCallback(
  (e: React.KeyboardEvent<HTMLTextAreaElement>): boolean => {
    if (!(e.metaKey || e.ctrlKey)) return false;
    if (!e.shiftKey || e.altKey) return false;
    if (e.key.toLowerCase() !== "d") return false;
    if ((e.nativeEvent as KeyboardEvent).isComposing) return false;
    e.preventDefault();
    const now = new Date();
    const mo = String(now.getMonth() + 1).padStart(2, "0");
    const d = String(now.getDate()).padStart(2, "0");
    const hh = String(now.getHours()).padStart(2, "0");
    const mm = String(now.getMinutes()).padStart(2, "0");
    const stamp = `${mo}-${d} ${hh}:${mm}`;
    insertMarkdownAtCursor("wrap", stamp, "");
    return true;
  },
  [insertMarkdownAtCursor],
);
```

- modifier check `meta/ctrl + shift && !alt`：避开既有 ⌘D（duplicate-
  line）/ ⌘⇧K（delete-line）/ ⌘B/I/U（bold/italic/strike）等家族
- 复用 `insertMarkdownAtCursor("wrap", stamp, "")` — 空 suffix 让"插入
  字符串到 cursor 位置"语义自然落入既有 wrap-mode 实现（空选区 + 空
  suffix → cursor 落 stamp 末尾）
- preventDefault 吃 browser ⌘⇧D 默认（部分 OS = "bookmark all tabs"）

#### 2. onKeyDown chain 双 textarea 均接入

- split 模式 textarea 链（line 13734）紧贴 `handleDetailCodeBlock`
  之后
- 纯 edit 模式 textarea 链（line 14197）紧贴 `handleDetailTabIndent`
  之后

两 textarea 均通过 `detailEditorRef` 共享同实现，但 onKeyDown 链是各
自独立的（既有架构）—— 新 handler 须显式接入两处。

## Key design decisions

- **短形 `MM-DD HH:MM`（无 year）vs 全形 `YYYY-MM-DD HH:MM`**：与既
  有 📅 toolbar 按钮互补差异化定位。短形适合 inline 标注（年内 year
  obvious 省 5 字符）；全形适合独立段落 ts / 跨年场景 / 与 [snooze:]
  marker 协议同形配对
- **⌘⇧D 选键**：⌘D 已被 duplicate-line 占；⌘⇧D 在 VS Code / Sublime
  / JetBrains 等 IDE 多用于"插日期 snippet"，与 owner 心智匹配。预防
  ⌘⌥D 留给"删除当前行向上"等未来扩展
- **`insertMarkdownAtCursor("wrap", stamp, "")` 复用**：避免新建第三
  个 helper（insertDoneLineAtCursor / insertCurrentTimeAtCursor 已两
  个独立 helper）— 空 suffix wrap 模式天然实现"插入到 cursor 位置 +
  cursor 落字符串末尾"
- **不加 toolbar 按钮**：既有 📅 toolbar 已 cover 全形入口；本短形
  shortcut 偏 power user 用 — keyboard-only 入口避免 toolbar 视觉过
  载。owner 想要短形多了走 toolbar 也成立但当前 1 个 toolbar 按钮 +
  1 个 shortcut 已是合理 cover
- **双 textarea 链都接**：split 模式和纯 edit 模式是两套独立 onKeyDown
  chain（既有架构决定），新 handler 必须显式两处接入。一致性测试由
  `tsc + vite build` clean + 手测两模式覆盖
- **不写 unit test**：纯 keyboard handler 调既有 `insertMarkdownAtCursor`
  helper（已 production 验证）+ Date 字符串拼接。GOAL.md "meaningful
  tests only" 规则下不引装饰性测试。`tsc` + `vite build` clean 即够

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.24s)
- 后端无改动 — 纯前端 keyboard handler
- 手测：split 模式 detail.md 编辑 → ⌘⇧D → 看 cursor 位置插 `MM-DD HH:MM`
  → 切纯 edit 模式 → ⌘⇧D → 同效；既有 ⌘D（duplicate-line）/ ⌘⇧K
  （delete-line）不受影响
