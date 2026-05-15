# detail.md 编辑器底部「行 N / 共 M」光标状态栏

## 背景

TODO 上 auto-proposed 一条："detail.md 编辑 textarea 底部显光标行号 / 总行数（『行 N / 共 M』）：调试 markdown 时方便找位置，与 IDE 状态栏同体验。"

detail.md 写多了（300+ 行 / 多段表格 / 嵌入图）时，"我现在敲在哪一行" / "保存前总长多少行" 是高频信号。VSCode / Sublime / IntelliJ 都用底部状态栏暴露光标行号 —— 写代码 / 写长 markdown 的肌肉记忆都期待这条信息。

既有底部状态行已有 ● 未保存 + 字数 counter 两个 chip；本 iter 在它们之间插入「行 N / 共 M」。

## 改动

### `src/components/panel/PanelTasks.tsx`

#### state + 重置

```ts
const [detailCursorPos, setDetailCursorPos] = useState<number>(0);
useEffect(() => {
  if (editingDetailTitle === null) setDetailCursorPos(0);
}, [editingDetailTitle]);
```

编辑器关闭 → 重置 0 防下次打开新任务闪烁旧光标值。

#### textarea 事件（两处：edit 模式 + split 模式 textarea）

```tsx
onChange={(e) => {
  setEditingDetailContent(e.target.value);
  setDetailCursorPos(e.target.selectionStart);
}}
onSelect={(e) => setDetailCursorPos((e.target as HTMLTextAreaElement).selectionStart)}
onKeyUp={(e) => setDetailCursorPos(e.currentTarget.selectionStart)}
onClick={(e) => setDetailCursorPos((e.target as HTMLTextAreaElement).selectionStart)}
```

四个事件交叉覆盖各种 cursor 移动方式：
- `onChange` —— 打字 / 删除（selectionStart 自然在新位置）
- `onSelect` —— 选区变更（包括 arrow keys + 拖选）
- `onKeyUp` —— `onSelect` 在某些浏览器对纯 cursor 移动（无 selection）不触发，keyup 兜底
- `onClick` —— 鼠标点击定位（onSelect 同样不一定触发）

冗余覆盖比"少一个 trigger 导致 chip 滞后"代价低；setState 同值 React 内部自然 short-circuit 不会引入 re-render。

#### chip 渲染

在 ● 未保存（line 6177）与 字数 counter（line 6207）之间：

```tsx
{detailViewMode !== "preview" && (() => {
  const cursor = Math.max(0, Math.min(detailCursorPos, editingDetailContent.length));
  const before = editingDetailContent.slice(0, cursor);
  const line = before.split("\n").length;
  const total = editingDetailContent.length === 0 ? 1 : editingDetailContent.split("\n").length;
  return (
    <span style={{ marginLeft: dirty ? undefined : "auto", ... }}>
      行 {line} / 共 {total}
    </span>
  );
})()}
```

#### 字数 chip marginLeft 修正

原 `marginLeft: dirty ? undefined : "auto"` 在 edit / split 模式与新加的"行号 chip" 冲突（两 chip 同时持 `auto` 会让 flex 把 free space 平分，布局错乱）。改为：

```tsx
const spacerOnSelf =
  detailViewMode === "preview" &&
  editingDetailContent === editingDetailOriginalRef.current;
// 只有 preview 模式 + clean 时 字数 chip 自己成为右推 spacer
```

- edit/split + dirty：● 未保存抢 auto，行号 / 字数 不抢
- edit/split + clean：行号 chip 抢 auto，字数不抢
- preview + dirty：● 未保存抢 auto（无 行号 chip），字数不抢
- preview + clean：字数 chip 抢 auto（无 行号，无 ●）

## 关键设计

- **仅 edit / split 模式渲染**：preview 是纯渲染态没有光标概念；强行显"行号"误导用户。`detailViewMode !== "preview"` gate 自然处理。
- **1-indexed 行号**：与 IDE 习惯（VSCode / IntelliJ / vim）对齐 —— 没有"行 0"。`split("\n").length` 天然 1-indexed。
- **total = 1 when empty**：空文本时 `split("\n").length === 1`（空字符串 split 后是 `[""]`）—— 不需特判，但加了显式 0-length 短路防边界 bug。
- **cursor clamp**：`Math.max(0, Math.min(cursor, content.length))` —— React 异步 batch + 删字时 selectionStart 短暂 > value.length 的边界，clamp 后行号永远合法。
- **muted gray + monospace**：与既有 ● 未保存 / 字数 counter 配色 + 字体一致，让三个 chip 在视觉上"成一组"。
- **4 个事件覆盖光标移动**：onChange + onSelect + onKeyUp + onClick —— `onSelect` 不覆盖"纯 cursor move"（无 selection 时） / `onKeyUp` 不覆盖鼠标点击。多事件冗余比"少一个 trigger 卡 chip"代价低；React 内部 setState 同值 short-circuit。

## 不做

- **不显列号**：行号 + 列号是 IDE 完整体；写 markdown 的 80% 是要"行级定位"（找代码块 / 表格段 / 跳到某行编辑）。列号增 chip 长度（"行 5 列 12 / 共 42"）但价值低。
- **不点击行号 → 跳行**：cool 但需要"go to line" modal 输入框 + textarea 跳光标 + scroll into view 一套，复杂度远大于"展示"。等真有用户诉求再做。
- **不显 selection length**："已选 N 字符"对长选区有用，但当前 detail.md 工作流是写 + 改而非大段 select 操作。等真有诉求再加。
- **不写测试**：纯字符串 split / slice，逻辑 5 行；既有 `formatRelativeAge` 等小工具也无测试。视觉验证即可。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.27s
- 改动 ~80 行（state + useEffect 8 + 两 textarea 事件 28 + chip 渲染 30 + 字数 chip marginLeft 修正 14）；既有 ● 未保存 / 字数 / banner 渲染路径不变。

## TODO 状态

5 条候选 auto-proposed 已完成 4 条（其中 1 条 stale 移除），余 2 条留池：
- mini chat 顶部上下文 token 提示 chip
- PanelChat 顶部「📌 钉住会话」 chip 计数

## 后续

- "点击行号跳行" `g g` 风快捷键：与 IDE Cmd+G 同体验。复杂度 +1 但价值高。
- 显当前段落 / heading 上下文："行 47 ▸ ## 实施步骤" —— 让长 detail 浏览有"自己在哪一节"的全局感。
- light-status-bar grouping：行号 + 字数 + 未保存合并成一个"状态条" semantic unit（如下圆角条），与编辑器边界更内聚。
