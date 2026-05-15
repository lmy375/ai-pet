# PanelDebug 快照加入"任务状态"段

## 背景

上轮把 task_stats 接到 PanelDebug 顶部 strip 后，发现快照 markdown 仍不含任务状态 —— issue triage 时 maintainer 知道"app v0.1.0 schema v4，spoke 5 silent 3 error 2"，但不知道"是否有 17 条 pending、3 条逾期"。任务积压本身是高信号的诊断线索。

`buildDebugMarkdownSnapshot` 加一段输出已有的 `taskStats` 即可（state 已挂 30s 轮询，免新增数据源）。

## 改动

`src/components/panel/PanelDebug.tsx`：

`buildDebugMarkdownSnapshot` 在 `## 工具缓存` 之前、`陪伴 N 天` 之后插一段：

```
## 任务状态
- 待办: N
- 逾期: N
- 今日完成: N
- 出错: N
- 今日取消: N
```

`taskStats === null`（还在 fetch / 旧 backend）→ 整段跳，与环境段同模式。

useCallback deps 加 `taskStats`。

## 不做

- 不加"任务标题列表"：那是 `/tasks` 的活；snapshot 是数字汇总
- 不另起 fetch：复用既有 30s 轮询的 state；snapshot 拿当下快照即可
- 不动 strip / 其它 consumer：单一 SoT 已稳

## 验收

- `npx tsc --noEmit` ✅
- 「调试」tab 点"复制快照" → markdown 含 `## 任务状态` 段 + 5 个数字
- 全 0 → 仍渲染段（让缺席本身可见 = 真没债 vs fetch 失败）

## 完成

- [x] buildDebugMarkdownSnapshot 嵌任务状态段 + deps
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
