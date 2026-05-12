# 任务列表"导出全部 MD"按钮

## 需求

bulk toolbar 已有"复制为 MD"，但走 selected 集合 —— 用户得先 click 一次"全选可见"
再 click 一次"复制为 MD"。复盘 / 周回顾的高频场景（"把当前任务清单贴到周报"）
要两步太啰嗦，缺一个一键直出按钮。

## 实现

### 用 visibleTasks 而非 selected

PanelTasks 已经维护了 `visibleTasks = sortedUnfinished + sortedFinished`，
是应用搜索 / tag / due / priority 过滤后的最终列表。新 handler 直接读这个数
组，不需 selected 选中态。

```ts
const handleExportAllVisibleAsMd = useCallback(async () => {
  if (visibleTasks.length === 0) {
    setBulkResultMsg("当前过滤下没有任务可导出");
    return;
  }
  const header = `# 任务导出（${N} 条 · ${date}）\n\n`;
  const body = visibleTasks.map(t => formatTaskAsMarkdown(t)).join("\n\n");
  await navigator.clipboard.writeText(header + body);
  setBulkResultMsg(`已导出 ${N} 条到剪贴板`);
}, [visibleTasks]);
```

复用现有 `formatTaskAsMarkdown`（不带 detail.md / history，避免一次拼 N 个文件
读 IO 让 UI 卡住，与 bulk 路径一致）。复用 `bulkResultMsg` 现有的反馈通道，
4s 自清。

### 函数顺序

`handleExportAllVisibleAsMd` 必须放在 `visibleTasks` 之后定义 —— TDZ 限制。

### 按钮位置

放在搜索行的过滤计数 chip 旁边（同 `s.searchClearBtn` 样式），无论是否有
filter 都常驻渲染，让位置稳定。无任务时 disabled 灰态。文案 `📋 导出 MD (N)`
显当前要导出的条数。

tooltip 在 filtersActive / 无 filter 两种态下文案不同，让用户清楚导出的是
"过滤后的 N 条"还是"全部 N 条"。

## 验证

- `npx tsc --noEmit` clean
- 行为：
  - 无 filter，10 条任务 → 按钮显 `📋 导出 MD (10)` → click → 剪贴板出大 md
    含 header + 10 段 `## title`
  - 加搜索 / tag → 按钮 N 跟着变 → click → 只导出当前可见的过滤集
  - 0 条 → disabled 灰态，hover tooltip 仍显
  - bulkResultMsg 4s 反馈 → 自动清

## 不在本轮范围

- 不带 detail.md：用户复盘场景常常希望看到详情笔记的，但单次导出 50 条任务
  各 read detail 是 50 次 IO 串行，UI 至少卡 2-3s。如果用户反馈"我要详情"
  再扩成 batch 后端命令一次返回所有 detail
- 不带 history：与 bulk 路径同决策；audit log 信息密度低，导出价值不高
