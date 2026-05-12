# PanelDebug 快照对比 diff

## 需求

PanelDebug 上一轮加了"📋 导出快照 MD"。现在用户想做"现在 vs 30 分钟前
状态对比"还得手动两次导出 + diff 工具。给 panel 内置抓 A → 对比现在 B
的最小路径。

## 实现

`src/components/panel/PanelDebug.tsx`：

- 新 state：
  - `snapshotA: string | null` —— 抓存的 A 时刻快照
  - `snapshotATs: string` —— A 抓取时间戳（show in tooltip）
  - `compareDiff: string | null` —— 当前 diff 渲染内容
- handlers：
  - `handleCaptureSnapshotA()`：snapshotA = buildDebugMarkdownSnapshot()
    + 时戳；同时清旧 compareDiff
  - `handleCompareSnapshot()`：拿当前 buildDebugMarkdownSnapshot() 作 B
    → 用 Set-based 行级 diff（仅 A / 仅 B / 共有计数）→ 拼 markdown：
    ```
    # 调试快照对比
    - A: <ts>
    - B: <ts>
    - 共有 N · 仅 A: X · 仅 B: Y

    ## 仅 A 出现（被移除 / 已变化）
    ```diff
    - line1
    - line2
    ```
    ## 仅 B 出现（新增 / 已变化）
    ```diff
    + line3
    + line4
    ```
    ```
  - `handleCopyCompareDiff()`：写剪贴板
  - `handleClearSnapshotCompare()`：清 A + 关闭 diff 视图
- toolbar 加按钮：
  - "📸 抓快照 A" / "📸 重抓 A"（snapshotA 非空时文案变）
  - 条件按钮（snapshotA 非空）："🔀 对比 A → 现在"（蓝色 tint）+ "清 A"
- 新 diff 渲染区：放在 toolbar 下方，`pre`+`code` 风格 + 11px monospace +
  maxHeight 320 + overflow auto；顶部 row 有"📋 复制 diff" / "✕ 关"按钮

## 为什么不用 jsdiff

snapshot 格式是结构化 markdown（一行一个 key:value），LLM stats 等数据
天然按行落地。set-diff（不保位置）已能完整答"哪几个值变了"——只看每
行内容，无需 unified diff 的位置追踪。少一个 npm 依赖 ~30KB minified。

如果后续需要顺序 diff（如对比 prompt / speech_history 之类有序流），再
引入 jsdiff 不晚；目前 stats 用例 set-diff 足。

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 点 📸 抓快照 A → 按钮文案变"重抓 A"+ "🔀 对比" + "清 A" 出现
  - 等几分钟（让 cacheStats / mood / speech 等变化）→ 点 🔀 → 下方 pre 区
    显 diff（仅 A 段 / 仅 B 段 / 共有计数）
  - 完全一致时 → "> 两次快照完全一致 —— 这段时间没有可观测变化"
  - 点 📋 复制 diff → 剪贴板得整段 markdown，可贴 GitHub issue
  - 点 ✕ 关 → 关闭 diff 视图，A 仍保留可再点 🔀
  - 点 清 A → 重置全部对比 state
  - 切到别的 tab 再回来 → A 保留（state 在组件内，未持久化但 session 内
    稳定）

## 不在本轮范围

- 没存到 localStorage：跨重启对比"昨天 vs 今天"语义不强；A 应是用户"我
  现在要 baseline"主动决定的时刻
- 没做多 A 对比（A / B / C 时间线）：UI 复杂度跳一档，本轮先保证最常用的
  二态对比
- 没引入 jsdiff：见上节说明；后续 prompt 流 diff 需要时再加

## TODO 池

清空后按规则 #1 自主提出 5 条新需求。

## TODO 池新提案

1. ChatMini hover 显已 mark NOW 任务列表
2. PanelTasks task title #tag inline 高亮
3. PanelChat session ⑂ fork 按钮
4. PanelMemory consolidate 进度 + cancel
5. PanelDebug 统计窗口快速切换 1d / 3d / 7d
