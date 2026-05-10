# Debug 页 Tab 与统计 UI 优化

> 对应需求（来自 docs/TODO.md）：
> Debug 页目前做的太乱了，首先将应用日志与 LLM 日志用单独的 Tab 分好。剩余的关于
> 各种统计信息的页，优化展示 UI，现在没法看。但这块不要花太多时间反复弄，差不多就
> 行，毕竟只是调试用的。

## 现状

- `src/DebugApp.tsx` 已经有「应用日志」/「LLM 日志」两个 tab —— 第一项需求其实已经成立。
- 「应用日志」tab 直接挂的是 `PanelDebug`，里面把 chip 条 + 工具栏 + 各类统计 + 日志窗
  全塞在一个垂直滚动里，密集度高，统计都挤在 chip 条一行又长又细。

## 改动

1. 新增第 3 个 tab「统计」，在 `DebugApp` 里挂新组件 `PanelDebugStats`。
2. `PanelDebugStats` 自己 `invoke('get_debug_snapshot')` 拉一次数据，按主题分卡片呈现：
   - 心情与陪伴：companionship_days、今日 / 本周 / 累计 speech 数。
   - LLM 决策：spoke / silent / error 三态比例 + 失败计数。
   - 环境感知：env tool 调用比例 + 分项（window / weather / events / memory_search）。
   - prompt 倾向：克制 / 引导 / 平衡 / 中性四桶分布。
   - cache：env tool 调用命中率。
   - mood tag：[motion: X] 前缀遵守率。
   每个卡片左上是标题、右上是「重置」按钮（如适用），中间一行大数字 + 一行解释。
3. `PanelDebug` 现状不动 —— 它还要支撑工具审核 modal、立即开口按钮等高频运维入口；
   按用户「差不多就行」的指示，这一轮只在 DebugApp 增加分流出口，不做大手术。

## 非目标

- 不动 PanelDebug 的内部结构（含其内嵌的 chip 条）—— 那块要彻底清需要拆 IPC 命令，
  本轮不值得。
- 不写新的 backend 命令 —— 复用 `get_debug_snapshot`。
