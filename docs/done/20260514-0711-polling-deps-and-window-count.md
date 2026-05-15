# `usePollingState` 加 `deps?` 参 + 迁 speechWindowCount

## 背景

上轮迁 hourlyBuckets 时跳过 `fetchWindowCount`（speech window 计数），因为它依赖 state `speechWindowDays`，切换 3/7/30 天时需立即 refetch，hook 之前没暴露这能力。

加 `deps?: unknown[]` 参数：deps 变化时除了 polling 周期外，额外触发立即 refresh。`usePollingState` 内部 `useEffect` 把 `...deps` 拼到自己的 dep 数组里 —— deps 元素变 → effect 重跑 → refresh + 重启 interval。

## 改动

### `src/hooks/usePollingState.ts`

加 4 个 overload（覆盖 with/without initial × with/without deps）+ 实现：

```ts
export function usePollingState<T>(
  fetcher: () => Promise<T>,
  intervalMs: number,
  initial?: T,
  deps?: ReadonlyArray<unknown>,
) { ... }
```

useEffect 改：

```ts
useEffect(() => {
  void refresh();
  const id = window.setInterval(() => void refresh(), intervalMs);
  return () => window.clearInterval(id);
  // eslint-disable-next-line react-hooks/exhaustive-deps
}, [intervalMs, refresh, ...(deps ?? [])]);
```

`...deps` 长度由 caller 决定，但同一组件每次 render 长度恒定（caller 传同一个数组形状），符合 rules-of-hooks 的"deps 数组长度不变"前提。仍加 eslint-disable 因为 lint 工具看不穿 spread 的稳定性。

### `src/components/panel/PanelDebug.tsx`：迁 speechWindowCount

```ts
const { data: speechWindowCount } = usePollingState(
  async () =>
    await invoke<number>("get_speech_count_days", { days: speechWindowDays }),
  30_000,
  0,
  [speechWindowDays],
);
```

切 days → 立即 refetch + setInterval 重启（与原 useEffect 行为等价）。catch 走 hook 静默。

## 不做

- 不迁 fetchMute / fetchLogs：上轮已讨论的设置/流式特例
- 不写 vitest

## 验收

- `npx tsc --noEmit` ✅
- 「调试」tab 切 3/7/30 天 chip → speech count 立即变化
- 30s 自动刷新仍正常

## 完成

- [x] usePollingState: 加 deps 参 + 4-way overload
- [x] PanelDebug.tsx: speechWindowCount 接入 hook
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
