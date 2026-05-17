# detail.md 编辑器 ⌘⇧K 删除当前行（iter #327）

## Background

detail.md textarea 已有 ⌘D 复制当前行（iter Cβ-ish）+ ⌘L 选中当前行
（iter #316）。但缺最常用的"删除当前行"快捷键 — owner 想砍掉一行（删
错的项 / 老的 marker / 注释）只能：Home → ⇧↓ → Delete 三步键序，或者
鼠标三连击 → Delete。

本迭代加 ⌘⇧K / Ctrl+⇧+K 删除当前行（VS Code "Delete Line" 习惯），与
既有 ⌘D / ⌘L 同 IDE-like 行操作集群形成"复制 / 选中 / 删除"完整三角。

## Changes

### `src/components/panel/PanelTasks.tsx`

- 新 callback `handleDetailDeleteLine = useCallback(...)`:
  - 命中 `e.metaKey || e.ctrlKey` + `e.shiftKey` + 无 alt + key=='k' +
    非 IME composing
  - 算第一行行首 / 最后一行行尾（与 ⌘L 同算法）
  - 删除范围 inclusive 末尾 `\n`（让行数真减；否则只清内容留空行）
  - 末行兜底：`indexOf('\n', end) === -1` → 删到 `value.length`
  - 新光标落到 `firstLineStart`（删后该位置 = 下一行行首；末行被删时
    Math.min 落到末尾，与 VS Code 同模式）
  - rAF + setSelection 同 ⌘D / ⌘L pattern
- 两 textarea onKeyDown 块（edit + split mode）都接入：
  `if (handleDetailDeleteLine(e)) return;` 在 ⌘L select-line 之后
- placeholder hint 文案补 `⌘⇧K 删除当前行` 让 owner 发现新快捷键
- ⌘/ cheatsheet modal detail-editor 段加新条 `⌘⇧K` →
  `删除当前行（VS Code「Delete Line」）`

## Key design decisions

- **⌘⇧K 而非 ⌘K**：与 ⌘K 跳 task palette（全局）/ ⌘K 跳到任意 task
  detail 冲突；⌘⇧ 修饰让 textarea 内行删除独立。这也符合 VS Code 同
  shortcut。
- **inclusive 末尾 `\n`**：删除范围 `[firstLineStart, nextNl+1)` 让删
  后行数真减。若 exclusive 末尾 `\n` 会留个空行 — 不是删除行的语义。
  末行特殊：无下一个 `\n` → 删到 EOF，连前一行的 `\n` 一起删才行 —
  但本实现保留前一行 `\n`（光标落到 firstLineStart Math.min 即 EOF）；
  这与 VS Code 行为对齐（删末行后光标停在新末尾）。
- **`!e.shiftKey || e.altKey` 守卫**：需 shift（区分 ⌘K = palette） +
  禁 alt（让位 ⌘⌥K 等未来扩展）。modifier cluster 留扩展空间。
- **handler 顺序：bracket pair → list-continue → duplicate → select →
  delete → save**：按"字符级 intercept 最高优先级 → IDE 行操作 → 保存"
  分层。⌘D / ⌘L / ⌘⇧K 都是 modifier+letter，互不冲突。
- **placeholder + cheatsheet modal 双重发现路径**：modal 是显式查询入
  口，placeholder 是 hint 浮现入口 — owner 第一次进编辑器看 placeholder
  即可见快捷键列表。
- **不引入 unit test**：键盘事件 + textarea selection 在 jsdom 难稳；
  既有 ⌘D / ⌘L 同型行为也未单测；通过 vite build + 真实交互验证。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.19s)
