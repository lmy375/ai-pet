# detail.md 编辑器底部 ☑ checklist 进度 chip

## 背景

TODO 上 auto-proposed 一条："detail.md 编辑器底部「☑ 已完成 N / 共 M」checkbox 进度 chip：扫 `- [ ]` / `- [x]` 行计数，让 owner 不必数勾选数也能看到完成度。"

detail.md 是 owner / 宠物轮流写"任务进度笔记"的地方。recently shipped toolbar `✓ 完成行` 按钮 + 既有 `☐ 待办` 按钮鼓励 owner 用 GFM checklist 跟踪子步骤；renderer 已识别 `- [x]` 渲为 disabled checkbox + line-through。

但当 detail.md 有 5-15 条 checklist 时，owner 难一眼判断"我完成了多少 / 还剩多少"—— 需要手动数。底部状态栏已经显 ● 未保存 / 行 N / 共 M / 字数，自然该有第 4 个 chip 显 checklist 进度，让"清单完成度"成为一眼可见的反馈。

## 改动

### `src/components/panel/PanelTasks.tsx`

在「行 N / 共 M」chip 之后、字数 counter 之前渲染：

```tsx
{(() => {
  const lines = editingDetailContent.split("\n");
  let total = 0;
  let done = 0;
  for (const line of lines) {
    const m = line.match(/^\s*- \[([ xX])\] /);
    if (m) {
      total += 1;
      if (m[1] !== " ") done += 1;
    }
  }
  if (total === 0) return null;
  const allDone = done === total;
  return (
    <span
      style={{
        fontSize: 10,
        color: allDone
          ? "var(--pet-tint-green-fg)"
          : "var(--pet-color-muted)",
        fontWeight: allDone ? 600 : undefined,
        fontFamily: "'SF Mono', 'Menlo', monospace",
      }}
      title={
        allDone
          ? `全部 ${total} 条 checklist 都已勾完 ✓`
          : `本 detail.md 含 ${total} 条 GFM checklist；已勾 ${done} 条。点工具栏 ☐ 加新条 / ✓ 完成行加『做完一条 + 时间戳』。`
      }
    >
      ☑ {done} / 共 {total}
    </span>
  );
})()}
```

## 关键设计

- **total === 0 不显**：无 checklist 时 chip 是噪音 —— 多数 detail.md 是纯进度笔记 / 长描述，不是 todo list。`return null` 让状态栏精简。
- **正则 `^\s*- \[([ xX])\] ` 与 `toggleEditChecklistLine` 同形**：两者识别同一组 `- [ ]` / `- [x]` / `- [X]` 三种形态，避免"用户切了 X → 计数没动"的边界 mismatch。
- **all-done 时变绿 + 加粗**：与既有「全部勾完」语义反馈一致（dueChip 等其它"达成"信号都用 green tint）。鼓励 owner 把清单做完 —— 视觉奖励而非纯信息。
- **嵌在 行号 + 字数 之间**：状态栏内顺序：[● 未保存] [行 N / 共 M] [☑ N / 共 M] [N 字]。「过程性」chip 居中，「内容统计」chip 居尾。
- **不参与 marginLeft auto spacer**：状态栏的"右推 spacer" 由更早的 dirty / 行号 / 字数 chip 之一承担（取决于哪些渲染）；checkbox chip 不抢这个 role，避免多 auto 破坏布局。
- **不与既有 ✓ 完成行 / ☐ 待办 按钮重复**：那两个是"插 marker 的输入工具"，本 chip 是"看 marker 进度的反馈工具"。input / output 两个方向，互补。
- **不在 preview 模式下隐藏**：preview / edit / split 三态都显 —— 渲染态下 owner 想看进度同样合理。仅与 cursor 强相关的「行号」chip 才 preview 隐。

## 不做

- **不显百分比**："☑ 3 / 共 5" 比 "60%" 直观（用户更关心"还剩几条"而非比例）；百分比反而需要心算"5×60%"。N/M 文本短。
- **不接 click → 跳到第一条未勾**：cool but cursor-jump 在 markdown textarea 里需要按 offset 算回 selectionStart，复杂度大。等用户真问起再做。
- **不写"剩余 N 条" 单独 chip**：信息冗余（owner 看 N/M 一眼能算）；多 chip 反噪音。
- **不写测试**：纯字符串 split + regex match，逻辑 10 行；既有 `formatRelativeAge` 等小工具也无测试，视觉验证（写多条 [ ] / [x] 看 chip 数字变化）已足够。

## 验证

- `npx tsc --noEmit` ✓ 0 error
- `npx vite build` ✓ 524 modules, 1.20s
- 改动 ~40 行（chip 渲染 35 + comment）；既有状态栏 dirty / 行号 / 字数 chip 路径不变。

## TODO 状态

6 条候选 auto-proposed 已完成 1 条，余 5 条留池：
- 任务详情视图模式 localStorage 持久
- session 下拉按月份分组折叠
- detail.md 打开自动滚到最新 `- [x]` 行
- 桌面 ChatPanel ⌘K 任务 ref picker
- TG /pinned 命令

## 后续

- chip click → 弹小 modal 列出所有 checklist 行 + 跳行 button（让 owner 在长 detail.md 里集中处理待办）。
- 跨任务"我有多少未完成 checklist"全局统计：周末复盘 / 看"自己积压了多少子步骤"用。
- "✓ 完成行" 按钮按下后自动 increment 一下"☑ N+1" + 短动画提示进度推进。
