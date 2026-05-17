# detail.md 编辑器 Tab / Shift+Tab 多行缩进（iter #368）

## Background

markdown 列表层级编辑当前没快捷键 — 写嵌套清单要手敲空格。VSCode /
Sublime / JetBrains / GitHub web editor 通用 Tab / Shift+Tab 缩进
是 IDE 通用习惯。本 iter 加上同 pattern，让 detail.md 列表层级
快速调整。

## Changes

### `src/components/panel/PanelTasks.tsx`

#### 1. `handleDetailTabIndent` callback（~line 2870）

```ts
const handleDetailTabIndent = useCallback((e): boolean => {
  if (e.key !== "Tab") return false;
  if (e.metaKey || e.ctrlKey || e.altKey) return false;
  if (isComposing) return false;
  // ... 实现 ...
}, []);
```

行为表：
| 选区 | Tab | Shift+Tab |
|------|-----|-----------|
| 无 | 光标位置插 2 空格 + 阻 native focus 跳走 | 本行 leading 2 空格 / 1 tab 削掉；无可削 noop |
| 单行 | 行首 +2 空格；选区漂 +2 | 行首 -2 空格；选区漂 -2（clamp 防过界）|
| 多行 | 每行 +2 空格；start += 2，end += 2 * 行数 | 每行 -2 空格（容忍 tab）；start clamp 防过界 |

设计要点：
- **INDENT = "  "（2 空格）**：markdown 紧凑场景。4 空格会让窄面板下
  缩进 ~30% 屏宽，2 空格更适合 detail.md 通常的 panel 宽度。
- **`probe = end > start ? end - 1 : end`**：选区末端正好在行起点
  时（即下行还没碰到字符）不应被算作覆盖那条 line，与 VSCode 行为
  一致。
- **Shift+Tab 容忍 `\t`**：legacy detail.md 可能含 tab 字符（其它编
  辑器写入的）— 反缩进时如果开头是 `\t` 削 1 个字符，否则按 2 空格
  削。`\t` 本身不会被本 handler 写入（Tab 一律插 INDENT）。
- **Shift+Tab 全行都没前导可削 → noop**：不动 value / selection，
  避免触发空 setState 引起的 cursor 闪烁。
- **`charsDeltaFirst` / `charsDeltaTotal` 双 delta**：start 仅受首
  行 indent 影响；end 受所有行总和影响。`Math.max(firstLineStart, …)`
  clamp 防反缩进时 start 漂到前一行。

#### 2. 接入 onKeyDown chain

两 textarea (split + edit mode) 都注入 `handleDetailTabIndent`，
位置在 `handleDetailBoldItalic` 之前 / `handleDetailLinkPopover` 之
前（IDE-style cluster：行操作 → wrap → link → save）。

#### 3. placeholder + cheatsheet

- 两 textarea placeholder 加 "Tab/⇧Tab 多行缩进"
- 速查 modal 增条目 `["Tab / ⇧Tab", "多行缩进 / 反缩进（...）"]`，
  放在 ⌘⇧L 之后 / ⌘B 之前（按"插入 → wrap → 行操作"梯度）

## Key design decisions

- **拦截 Tab 全部场景（含无选区）**：理论上无选区 Tab 在 form 内
  应跳焦下一个 input。但 detail 编辑器是 modal-like 全屏编辑，owner
  打字时几乎不会想"Tab 跳到下一个 form field" — 改为 markdown 通用
  "插 indent" 更直觉。如果 owner 真想跳焦走鼠标 / 关闭编辑器（Esc）
  即可。
- **不引入"shift+enter = unindent then next line" 等 IDE 复合**：
  scope 守住 Tab / Shift+Tab 二态足够。复合操作（⇧Enter + auto-unindent
  list bullet）下次另开 TODO。
- **`Math.max(firstLineStart, …)` clamp**：反缩进时 start 距首行起
  点 < 2 字符场景。比如光标在行首 "  a"，shift+tab → "a"，start
  原本 = 0（已是首行起点），charsDeltaFirst = -2，理论 newStart =
  -2，clamp 到 0（= firstLineStart）。
- **不为单 fn 引 unit test runner**：项目无 .test.tsx 历史；
  Tab/Shift+Tab 是 IO + state ops 不是纯函数，build pass + 手测
  足够（每场景手测一次 +/-）。
- **`requestAnimationFrame` 内 refocus + setSelection**：与既有
  handleDetailSelectLine / handleDetailDeleteLine 同模式。React
  setState 后浏览器需要一帧 reflow 才能正确 set selectionStart/End。

## Verification

- `npx tsc --noEmit`（frontend）— clean
- `npx vite build`（frontend）— clean (1.27s)
- 后端无改动
