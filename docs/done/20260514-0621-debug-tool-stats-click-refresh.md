# PanelDebug "🛠 专用工具占比" strip 点击手动刷新

## 背景

上一轮把 "📊 任务状态" strip 做成可点击立即重 fetch。同 PanelDebug 上方的 "🛠 专用工具占比" strip 也是 30s 自动轮询 → 同样有"刚改完想立刻看"的需求。补齐一致性。

## 改动

`src/components/panel/PanelDebug.tsx`：

复用既有 `dedicatedToolStats` setter；加 `dedicatedToolStatsRefreshing` flag + `refreshDedicatedToolStats` 函数；strip 外层 div 加 `role="button"` / cursor / opacity 反馈。"🛠 专用工具占比（窗口 N）" header 在 refreshing 时改成 "🔄 刷新中"。

模式与 task_stats refresh 完全对称。

## 不做

- 不抽公共 `RefreshableStrip` 组件：仅 2 处使用，抽出需要传 children / state setter / fetcher 等 4-5 个 props，比就地复用更复杂
- 不动 30s 轮询节奏

## 验收

- `npx tsc --noEmit` ✅
- 「调试」tab 顶部 🛠 strip：cursor:pointer，点击立即重 fetch + 短暂 "🔄 刷新中" 反馈

## 完成

- [x] PanelDebug.tsx: dedicatedToolStatsRefreshing + refresh handler + strip onClick
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
