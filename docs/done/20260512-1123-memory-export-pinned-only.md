# PanelMemory 单段 export 加 "📌 pinned-only 子集"

## 背景

R94 引入 pin 系统后，用户在每段内挑了少数关键条目 pin 到顶。导出时常想"只把这段里我 pin 的几条给同事看"，但现有「📋 单段…」下拉只能整段导出。

## 改动

`PanelMemory.tsx` 顶部「📋 单段…」下拉重构：

- value 编码改为 `cat:<key>` / `pin:<key>` 两类。
- 用 `<optgroup>` 分两组：
  - **全段**：所有非空 category（行为不变）。
  - **📌 仅 pinned**：只列出至少有 1 条 pinned 的 category，文字 `📌 <label> (<pinCount>)`。
- 选 pinned-only 时：过滤 `pinnedKeys`，markdown 标题加 `· 📌 pinned` 后缀，msg 文案区分。
- 边界：若 pinnedKeys 在渲染后被改空导致 0 命中（race），显示"该段内还没 pin 任何条目"并 reset 下拉。

## 不做

- 不做"全 pinned 跨段导出" —— TODO 明确"段内 pinned items"，跨段需求暂未浮现。
- 不动 `exportMemoriesAsMarkdown` 全集 helper —— pinned 是 UI 偏好（localStorage），不该污染"全量导出"语义。
- 不写测试 —— 纯 UI 串联，pinnedKeys / categories 行为已由现有代码覆盖。

## 验收

- 下拉打开见两组 optgroup："全段" + "📌 仅 pinned"。
- 选段内有 pinned 的 cat → 📌 行出现，数字 = pinSet 命中数。
- 选 pin:<key> 后剪贴板内容仅含该段内 pinned items。
- 没 pin 的 cat 不出现在 📌 组里（避免空选项）。

## 完成

- [x] 写 PanelMemory.tsx
- [x] 移到 docs/done/
- [x] git commit
