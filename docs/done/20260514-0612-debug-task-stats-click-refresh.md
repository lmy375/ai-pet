# PanelDebug "📊 任务状态" strip 点击手动刷新

## 背景

任务状态 strip 30s 自动轮询，足够日常但 debug 时常常需要"刚改了任务 → 立刻看变化"。点击 strip 触发立即重 fetch 比等下一个 30s 周期顺手。

## 改动

`src/components/panel/PanelDebug.tsx`：

### state + handler

```ts
const [taskStatsRefreshing, setTaskStatsRefreshing] = useState(false);
const refreshTaskStats = async () => {
  if (taskStatsRefreshing) return;
  setTaskStatsRefreshing(true);
  try {
    const s = await invoke<TaskStats>("task_stats");
    setTaskStats(s);
  } catch {
    // 静默：旧 backend / 命令未注册 → 保持上一份数据
  } finally {
    setTaskStatsRefreshing(false);
  }
};
```

复用既有 `taskStats` setter；refreshing flag 防快速重复点。

### strip 改 button 样式 + onClick

外层 div 加 `cursor: pointer` + `onClick={refreshTaskStats}` + active 时降低 opacity 给反馈。"📊 任务状态" header 文字改成 "🔄 刷新中" 当 refreshing。

不抽 button 元素 —— strip 内含多个 span 子元素，外层 `<button>` 不语义化（嵌套 span 在 button 里），div + role="button" + onClick + title 即可。

## 不做

- 不加自动重 fetch 的"上次刷新时间"指示：30s 周期对用户而言"足够新"
- 不动 strip 视觉风格（仍是横条 chip 同款）：让"可点击"通过 cursor / hover 暗示，不强加 button 外框

## 验收

- `npx tsc --noEmit` ✅
- 「调试」tab 顶部 strip：cursor:pointer
- 改完一个任务（done / 创建）→ 点 strip → 数字立即更新
- 同 strip 连点几下 → refreshing 期间不重复 invoke（最早一次完成后才能再点）

## 完成

- [x] PanelDebug.tsx: taskStatsRefreshing state + refreshTaskStats fn + strip onClick
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
