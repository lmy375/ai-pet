# detail.md 编辑器 字数 chip 选区感知

## 背景

iter #198 给字数 chip 加了"〜M 词" word count。但选中文本时仍显总字数，与 IDE / Pages / Numbers / VSCode 等编辑器的"选中区即时反馈选 N 字"UX 不一致。

owner 想知道"我选了这段是多少字 / 多少词"时，必须心算或复制出来到别处计数。本 iter 让字数 chip 选区感知 —— 有 selection 时切到选区子串计数 + 视觉加重（accent 色 + 粗体 + "选 " 前缀）。

## 改动

### `src/components/panel/PanelTasks.tsx`

#### 1. 新 `detailSelectionEnd` state

```ts
const [detailSelectionEnd, setDetailSelectionEnd] = useState<number>(0);
```

与既有 `detailCursorPos`（= selectionStart）配对，让 chip IIFE 能算 selection 长度。

#### 2. 两个 textarea (edit / split 模式) 全 4 个事件 (onChange/onSelect/onKeyUp/onClick) 同步 setDetailSelectionEnd

```ts
onChange={(e) => {
  setEditingDetailContent(e.target.value);
  setDetailCursorPos(e.target.selectionStart);
  setDetailSelectionEnd(e.target.selectionEnd);  // 新
}}
onSelect / onKeyUp / onClick: 同模板加 setDetailSelectionEnd
```

`replace_all: true` 替换两个 textarea 处共 8 个事件 handler 改成 block-body 风格。

#### 3. 编辑器关闭时 reset

```ts
useEffect(() => {
  if (editingDetailTitle === null) {
    setDetailCursorPos(0);
    setDetailSelectionEnd(0);
  }
}, [editingDetailTitle]);
```

#### 4. 字数 chip IIFE 引入选区分支

```ts
const selStart = Math.min(detailCursorPos, detailSelectionEnd);
const selEnd = Math.max(detailCursorPos, detailSelectionEnd);
const hasSelection =
  selEnd > selStart &&
  selStart >= 0 &&
  selEnd <= editingDetailContent.length &&
  detailViewMode !== "preview";  // preview 无 textarea
const countSource = hasSelection
  ? editingDetailContent.slice(selStart, selEnd)
  : editingDetailContent;
// charCount / cjkCount / enWords / wordCount 用 countSource 算
// 颜色：hasSelection → accent；否则按 editCount (全文) 走原阈值配色
```

#### 5. 视觉态切换

- prefix `"选 "` 仅 hasSelection 时显
- color：accent / danger / longish / muted 优先级
- fontWeight：hasSelection || danger → 600；否则 default
- tooltip：选区时 `选区 X 字 / 共 Y 字` 标注同时含全文长度

## 关键设计

- **selStart / selEnd Math.min/max 防 selectionStart > selectionEnd**：浏览器一般保证 start ≤ end，但 IE / 某些 web view 在反向 selection（owner 从右往左拖选）可能反过来；min/max 兜底。
- **阈值配色仍按 editCount 全文走**：选了 100 字不该跳红 banner 误导 owner "整段超了" —— 阈值（2000 amber / 5000 red）是 detail.md 整体 token 预算信号。selection 仅给 char/word 计数，不参与阈值。
- **detailViewMode !== "preview" gate hasSelection**：preview 模式无 textarea，detailSelectionEnd / detailCursorPos 是 stale 历史值；不应进入 selection 分支。
- **wordCount 用 countSource 而非全文**：选区"〜M 词"对长英文段落（"选中 hello world..."）实用 —— 让 owner 知道选了多少 token。
- **accent 色 + 粗体强调 selection**：与 active filter chip 同视觉语言（accent border / fg），让"现在显的是 selection" 一眼可辨。
- **tooltip 选区时附"共 Y 字"全文长度**：让 owner 知道选了占多大比例 —— 写论文 / 长 note 时常用。
- **不重写阅读态 counter**：阅读态（detailViewMode === "preview"）没 textarea，无 selection 概念；保持既有行为。

## 不做

- **不为阅读态 counter 加 selection 支持**：阅读态显示的是 rendered markdown，浏览器 selection 横跨 React 渲染的多个 DOM 节点 —— 测算 selection char count 复杂（不是单 textarea value 的简单 slice）。Future iter 加 contentEditable 选区监听才能做。
- **不写自动 toast "选了 N 字"**：chip 已即时反馈，多余 toast 噪音。
- **不分行 / 列 selection（只看 char-range）**：单 textarea 是线性 string，selection 自然就是 [start, end) range；不需算"选了几行 几列"。
- **不在 ChatMini / PanelChat 等 textarea 同改**：本 iter 仅 detail.md editor scope；其它 textarea 没字数 chip，加 selection 感知也没目标 UI。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 1.21s
- 改动 ~80 行（new state + reset + 8 event handlers replace_all 35 + chip IIFE 选区分支 35 + 注释 10）。既有 editCount / cjkCount / enWords / longish / danger / spacerOnSelf / 阈值配色 / preview 模式判定 / detailCursorPos 链路完全不动。

## TODO 状态

剩 3 条留池：
- butler_task edit-schedule modal 扩支 every_weekdays
- PanelChat session bar item hover 1s 浮 "最近 3 条" preview
- ChatMini bubble click + ⌘ 复制单条

## 后续

- ⌘+点击 chip 复制 selection / 全文（"我选 200 字了想 paste 走"）。
- chip click 短暂闪一行 "选区 X 字" 大号 toast，便于直播 / 演示 / accessibility。
- detail.md 阅读态用 `document.getSelection()` 监听浏览器 selection 改变事件 (selectionchange listener)，扩 selection-aware 到 rendered markdown。
- "百分比" mode：selection 占全文 N% 显示，长文档复盘"我重点改了哪段" 直观。
