# detail.md 编辑器链接快速插入 popover（iter #365）

## Background

owner 键盘党插链接当前要走两步：
1. 点 toolbar 🔗 按钮（或没选区时 cursor 落 `[|]`）
2. 替换 url 占位符 + 切回 [ ] 改 label

VSCode / Notion 风的"⌘K 弹小窗一次性输完整 url + label"流程更键盘
友好。TODO 原定 ⌘K，但 detail.md 编辑器内 ⌘K 已绑定 task quick-find
palette（line 4374-4382）— 不能覆盖。改用 **⌘⇧L**（L = link 助记，
shift 与 ⌘L "select line" 区分修饰语义）。

## Changes

### `src/components/panel/PanelTasks.tsx`

#### 1. state（~line 4406，紧贴 palette state）

- `linkPopoverOpen` / `linkUrlDraft` / `linkLabelDraft`
- `linkSelectionRangeRef`: useRef 存"打开 popover 时的 selection
  [start, end]"。popover input focus 会清掉 textarea selection，提
  交时必须 fallback 到这个 ref，不能再读 textarea
- `linkUrlInputRef` / `linkLabelInputRef`: 用于选区策略下首焦点

#### 2. `handleDetailLinkPopover` keyboard handler

```tsx
if (!(e.metaKey || e.ctrlKey)) return false;
if (!e.shiftKey || e.altKey) return false;
if (e.key.toLowerCase() !== "l") return false;
```

- 捕获 textarea selection 存 ref
- selection 非空 → label 预填选区文 + 首焦点跳 url input
- selection 空 → label / url 都空 + 首焦点跳 label input
- 接入两 textarea (split + edit mode) 的 onKeyDown chain（与
  handleDetailBoldItalic 同位插）

#### 3. `commitLinkPopover` 提交

- url 空 → no-op（按钮 disabled，不应触发；防御）
- label 空 trim → fallback "link" 避免插出 `[](url)` 空 anchor
- 插 `[label](url)` 到 ref range（覆盖原选区）
- cursor 落 inserted 末尾
- 关 popover + 清 draft

#### 4. popover render（~line 13467，紧贴 palette）

fixed 全屏 overlay + 居中 panel：
- 标题 "🔗 插入链接"
- 副标题说明插入位置 / 协议
- Label input + URL input（label 也允许空 → fallback "link"）
- Enter 在 label 时：若 url 空 → 跳焦 url；非空 → commit
- Enter 在 url 时：url 非空 → commit；空 → no-op
- Esc 关
- 点 backdrop 关
- 按钮：取消 / 插入（disabled 至 url 非空）

#### 5. placeholder + cheatsheet

- 两 textarea placeholder 加 "⌘⇧L 插入链接"
- 速查 modal 增条目 `["⌘⇧L", "弹链接快速插入 popover（...）"]`

## Key design decisions

- **键不是 ⌘K**：TODO 原文要 ⌘K 但与既有 task palette 冲突。⌘⇧L
  是 mnemonic 最贴近的备选（L = link），shift 与既有 ⌘L "select
  line" 修饰扩展同字母键语义集群。placeholder + cheatsheet 都明
  确标 ⌘⇧L。
- **保留既有 🔗 toolbar 按钮行为不动**：那个是"占位符 pre-select"
  流（owner 想"先看到模板再改"心智），与新 popover 流（"我已经知
  道 url 是啥，一次性输完"心智）面向不同 workflow。两路径互补，
  不要相互替换。
- **selection range 存 ref 而非 state**：popover 打开后 input focus
  会清 textarea selection；如果走 state 则 re-render 会取最新（空）
  selection。ref 是"打开时刻的快照"，与 setEditingDetailContent 后
  插入位置精确对齐。
- **label fallback "link" 而非 disabled**：owner 可能想"我只关心
  url，label 随便"。空 label disabled 提交会让流程卡住；fallback
  "link" 让 commit 永远过，owner 事后自己改也行。disabled 仅作用
  于 url 空（没 url 没法生成合法 link）。
- **不持久化 popover 偏好**：每次开都是新 range + 空 draft，与既有
  toolbar 🔗 同模板 — 这是"一次性弹出"语义。
- **fixed overlay 而非 inline popover**：与 ⌘K palette 同风格，避免
  textarea 内 absolute 定位的坐标计算（textarea 内 cursor 像素位置
  需通过 mirror div 计算，复杂度高）。fixed 居中 + backdrop 也让
  popover 可读性最好。

## Verification

- `npx tsc --noEmit`（frontend）— clean（一次 TDZ 把 state 声明放到
  handler 之后修复）
- `npx vite build`（frontend）— clean (1.24s)
