# PanelApp overdue-count 接入 `usePollingState`

## 背景

上轮把 App.tsx pet 窗 taskStats 迁到 `usePollingState`。PanelApp.tsx 的 `overdueCount` 也是同模板，**但额外有一个"切到 任务 tab 立即 refetch"的 effect** 让用户在 PanelTasks 里 retry / cancel 改完任务后回到主面板能看到红点徽章同步。

`usePollingState` 暴露了 `refresh()`，调 caller 自行触发即可。tab-change effect 保留独立但用 hook 的 refresh 而非自定义 fetchOverdue。

## 改动

`src/PanelApp.tsx`：

把：

```ts
const [overdueCount, setOverdueCount] = useState<number>(0);
const fetchOverdue = useCallback(async () => {
  try {
    const n = await invoke<number>("task_overdue_count");
    setOverdueCount(n);
  } catch (e) {
    console.error("task_overdue_count failed:", e);
  }
}, []);
useEffect(() => {
  fetchOverdue();
  const id = window.setInterval(fetchOverdue, OVERDUE_POLL_MS);
  return () => window.clearInterval(id);
}, [fetchOverdue]);
useEffect(() => {
  if (activeTab === "任务") fetchOverdue();
}, [activeTab, fetchOverdue]);
```

换成：

```ts
const { data: overdueCount, refresh: refreshOverdue } = usePollingState(
  () => invoke<number>("task_overdue_count"),
  OVERDUE_POLL_MS,
  0,
);
useEffect(() => {
  if (activeTab === "任务") void refreshOverdue();
}, [activeTab, refreshOverdue]);
```

`OVERDUE_POLL_MS` 常量保留（30_000）。

行为差异：原 catch 写 `console.error`；hook 静默吞。与上轮 App.tsx 迁移同口径 —— 60s/30s 偶发失败不值得日志噪音。

## 不做

- 不动 PanelApp 主体逻辑：仅替换 polling 这条
- 不展示 refreshing 给徽章用：徽章不需要 loading 态（30s 自动 + tab 切换触发已经够"新鲜"）

## 验收

- `npx tsc --noEmit` ✅
- 「任务」tab 红点徽章自动 30s 刷新 + 切到任务 tab 立即 refetch + retry/cancel 操作后切回主面板时同步

## 完成

- [x] PanelApp.tsx: 删 fetchOverdue / useState / 30s effect，用 usePollingState；保留 tab-change refresh effect
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
