# App.tsx pet 窗 taskStats 接入 `usePollingState`

## 背景

上轮抽 `usePollingState<T>` 后，PanelDebug 两条 strip 都迁过去了。剩下 App.tsx pet 窗的 taskStats 60s 轮询是同模板的第 3 个 callsite，但**它需要 non-null initial**（`{ overdue: 0, done_today: 0 }`）—— 现有 hook 签名只返回 `T | null`。

加 overload 让 hook 同时支持 nullable 与 non-null 两种用法。

## 改动

### `src/hooks/usePollingState.ts`

加 overload：

```ts
export function usePollingState<T>(
  fetcher: () => Promise<T>,
  intervalMs: number,
): { data: T | null; refresh: () => Promise<void>; refreshing: boolean };
export function usePollingState<T>(
  fetcher: () => Promise<T>,
  intervalMs: number,
  initial: T,
): { data: T; refresh: () => Promise<void>; refreshing: boolean };
export function usePollingState<T>(
  fetcher: () => Promise<T>,
  intervalMs: number,
  initial?: T,
): { data: T | null; refresh: () => Promise<void>; refreshing: boolean } {
  const [data, setData] = useState<T | null>(initial ?? null);
  // ... 其余不变
}
```

initial 写入 `useState` 初始值。TS 重载让 caller 不传 initial 时 data 是 `T | null`，传了就是 `T`。

### `src/App.tsx`

把内联 useState + useEffect block：

```ts
const [taskStats, setTaskStats] = useState<{ overdue: number; done_today: number }>(
  { overdue: 0, done_today: 0 },
);
useEffect(() => { ...60s 轮询... }, []);
```

换成：

```ts
const { data: taskStats } = usePollingState(
  () => invoke<{ overdue: number; done_today: number }>("task_stats"),
  60_000,
  { overdue: 0, done_today: 0 },
);
```

`refresh` / `refreshing` 此处用不到（不需要点击立刻刷新），解构丢掉。

行为差异：原 catch 走 `console.warn(...)`；hook 统默吞。多数情况"60s 偶发 fetch 失败"不值得 console.log 噪音；与 hook 既有行为对齐。

## 不做

- 不暴露 onError 回调让 caller 决定打印：本次"统一静默"是合理默认，未来真需要再扩
- 不动 PanelApp.tsx overdue tab 红点：那是 fetchOverdue 走 30s + activeTab 触发显式 refetch 的复合逻辑，与简单 polling 不同模板，强抽反而失真

## 验收

- `npx tsc --noEmit` ✅
- 桌面 pet 窗 60s 自动刷新红 pill 行为不变
- 启动初次 fetch 完成前，taskStats 仍是 `{ overdue: 0, done_today: 0 }` 默认值（pill 不闪）

## 完成

- [x] usePollingState: 加 overload + initial 参
- [x] App.tsx: 删 useState/useEffect block，用 hook
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
