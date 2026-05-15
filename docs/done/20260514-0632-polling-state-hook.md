# 抽 `usePollingState` hook

## 背景

PanelDebug 上两条 strip（task_stats / dedicated_tool_stats）现在各自维护 3 个东西：
1. `useState<T | null>` data 容器
2. 30s 轮询 useEffect（mount + setInterval + cleanup）
3. `refreshing` flag + manual refresh fn（防快速重复点）

三件套结构一致，只 fetcher 和文案不同。第二条 refresh 上线后已经把模板复刻一份；如果 App.tsx pet 窗 pill 等再加 manual refresh，会有第三份。趁早抽 hook。

## 改动

### `src/hooks/usePollingState.ts`（新）

```ts
function usePollingState<T>(
  fetcher: () => Promise<T>,
  intervalMs: number,
): { data: T | null; refresh: () => Promise<void>; refreshing: boolean };
```

实现要点：
- 挂载时立即 fetch + 启动 setInterval；返回 cleanup 清 interval
- `fetcher` 通过 ref 保最新版（caller 传 inline lambda 不会反复 re-subscribe）
- `inFlight` ref 同步守门 dedupe 快速重复点（setState 异步不能 atomic check）
- fetcher throw 时静默吞错保持上一份 data（"不闪 null"）

### `src/components/panel/PanelDebug.tsx`

两处 strip 各自 ~30 行（state + effect + refresh fn）→ 6 行解构：

```ts
const { data: taskStats, refresh: refreshTaskStats, refreshing: taskStatsRefreshing } =
  usePollingState<TaskStats>(() => invoke<TaskStats>("task_stats"), 30_000);
```

行为完全等价：同 30s 轮询、同 refreshing flag、同静默错误处理。

## 不做

- 不迁 App.tsx pet 窗 taskStats：那条 initial 是 non-null `{ overdue: 0, done_today: 0 }`，hook 当前签名 initial = null。要支持 initial 参数得扩 generic 默认 / 改签名，本轮不动
- 不迁 envInfo（PanelDebug 顶部 app_version + schema fetch）：那是一次性 Promise.all 不轮询，不在模板里
- 不写 vitest：项目无 frontend test runner

## 验收

- `npx tsc --noEmit` ✅
- 「调试」tab 两条 strip 自动刷新 + 点击 refresh 行为不变
- 数字与之前等价

## 完成

- [x] hooks/usePollingState.ts 新建
- [x] PanelDebug.tsx: 两 strip 接入 hook（删冗余 effect / state / refresh fn）
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
