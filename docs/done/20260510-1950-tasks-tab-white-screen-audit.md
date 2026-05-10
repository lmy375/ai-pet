# PanelTasks 白屏：审阅 + 兜底 + 待真因数据

> 对应需求（来自 docs/TODO.md）：
> 排查并修复 Panel 任务页打开白屏的根本原因（上轮 ErrorBoundary 已兜底，本轮要找出真因）。

## 现状

上一轮已经在 `PanelApp` 加了 `TabErrorBoundary`，所以再出现「白屏」时
React 会把 `error.message + stack` 直接渲染在内容区。也就是：

- 如果是 React 渲染期 `throw`，用户会看见红框 + 报错文本 + 重试按钮，而不再
  看到一片空白。
- 如果是布局问题（高度坍塌、CSS 变量缺失、字体错乱），boundary 不会触发；
  仍可能看到空白。

## 本轮静态审阅结论

通读 `src/components/panel/PanelTasks.tsx`（3055 行）后，没有发现明确的
渲染期 throw 候选：

- `if (loading) return …`、`visibleTasks.length === 0` 都有空态分支。
- 所有 `parseInt` / `Date.parse` 都用了 `Number.isNaN` / `Number.isFinite`
  检查，遇到坏值返回 `null` / `0`。
- `localStorage.getItem` 全部包了 try-catch。
- `bucketFor` 是纯函数，分支覆盖 4 种返回值。
- `STATUS_BADGE[t.status]` 和 `BUCKET_LABELS[bucket]` 走 `Record<enum, T>`，
  TS 已确保 enum 完备。
- `s.headerClickable` / `s.rowCheckbox` 等 `s.*` 引用全部能在 1319 行的
  `const s = { ... }` 里找到对应键。
- `useMemo` 的 dep 数组全部正常（虽然 `bucketBoundaries` 的 `sortedFinished`
  每次都是新引用让 memo 形同虚设，但不会导致 throw）。

`tsc --noEmit` 干净，`vite build` 干净，`cargo check` 干净。

## 兜底已就位 + 等待用户反馈

- ErrorBoundary 已在 `PanelApp.tsx` 上线（commit `e054374`）。下次再次
  出现白屏时，用户截图 / 复述报错文字即可定位真因。
- 如果进一步白屏不再出现，说明上一轮的 focus-mode / privacy 字段瘦身
  顺带把根因清掉了（`tone.daily_block_stats!.peak_single_stretch_minutes`
  等深嵌套 `!` 已被一并删除，可能是源头之一）。

## 下一步触发条件

下一次用户反馈「仍白屏 + 出现 boundary 报错」时，按 boundary 显示的栈
回到对应行修复；boundary 不显且仍白屏时改去看 CSS / 布局。本轮先把
boundary + 静态审阅的进度落档，避免再花费时间空转。
