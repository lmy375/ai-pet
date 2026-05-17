# PanelTasks「📋 标题 (N)」复制全部可见标题按钮（iter #307）

## Background

PanelTasks 已有「📋 导出 MD」按钮把 visibleTasks 拼成完整 markdown
（含 metadata + 可选 detail.md）。但 owner 经常想做更轻量的导出 ——「我
想把任务清单粘到 Notion / Things / 另一个工具」只要标题列表即可，full
markdown 太重 + 含 [task pri=...] markers 等其它工具看不懂的元数据。

本迭代加「📋 标题 (N)」按钮 — 仅一行一标题（纯文本）复制到剪贴板。

## Changes

仅 `src/components/panel/PanelTasks.tsx`：

- 新 callback `handleCopyVisibleTitles`：
  - 空 visibleTasks → "当前过滤下没有任务可复制" toast，不真复制
  - 否则 `visibleTasks.map(t => t.title).join("\n")` 写入剪贴板
  - 成功 / 失败都通过 setBulkResultMsg 4s toast 反馈
- 按钮渲染：在「📋 导出 MD」之后插「📋 标题 (N)」，复用 `s.searchClearBtn`
  样式 + disabled when length === 0
- tooltip 分 filtersActive / 全集两种文案让 owner 知道当前 N 含义

## Key design decisions

- **不复用 handleExportAllVisibleAsMd**：那个走 detail.md fetch /
  formatTaskAsMarkdown / header 等较重路径；标题导出本质是 0-IO 轻量
  操作（visibleTasks 已在内存），单独 callback 让代码意图清晰 + 不挂
  Promise.all 等异步路径。
- **不附 metadata（status / priority / due / tags）**：owner 要 metadata
  会走「📋 导出 MD」；本按钮的差异化定位就是"轻"。N 行纯文本粘到任
  何文本输入框都成。
- **不含 filtersActive 文案区分**：原想 callback 内根据 filtersActive
  发不同文案，但 filtersActive 声明在该 callback 之后会触发 TS
  `used before declaration` —— 弃用方案，统一用「已复制 N 条标题」简单
  文案。tooltip 仍带 filtersActive 区分让 hover 时清楚 N 含义。
- **按钮位置紧贴「📋 导出 MD」**：两者语义近（都是 quick-export），
  position adjacency 让 owner 视觉上看到"轻 / 重两个出口"。

## Verification

- `npx tsc --noEmit` ✅
- `npx vite build` ✅ (1.24s)
