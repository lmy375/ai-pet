# PanelDebug hourlyBuckets 接入 `usePollingState`

## 背景

PanelDebug 还有几条 setInterval 轮询。其中 `hourlyBuckets`（今日 24 小时主动开口分桶 sparkline）最符合 hook 模板：

- 60s 自动 fetch
- 无外部 state 依赖
- catch 后什么也不做（与 hook 静默策略一致）
- 24 元素 array 初始值 = 全零

唯一特殊：原代码做了 shape 验证（`Array.isArray && length === 24`）才 set。
fetcher 内 throw 即可让 hook 静默吞，保留上一份 24-zero 数组 / 上一帧合法数据 —— 渲染端不会撞 length 错的 array。

## 改动

`src/components/panel/PanelDebug.tsx`：

```ts
const { data: hourlyBuckets } = usePollingState(
  async () => {
    const arr = await invoke<number[]>("get_today_speech_hourly");
    if (!Array.isArray(arr) || arr.length !== 24) {
      throw new Error("invalid hourly buckets shape");
    }
    return arr;
  },
  60_000,
  new Array(24).fill(0) as number[],
);
```

行为差异：原 catch 打 `console.error`；hook 静默。60s 周期偶发失败不值得日志噪音，与其它 polling 迁移同口径。

## 不做

- 不迁 `fetchWindowCount`（speechWindowCount）：依赖 state `speechWindowDays` 作为参数；切换天数时需要立即 refetch，hook 当前签名靠 fetcher ref 自然 catch 不到这种 dep 变化的"立即重 fetch"语义。要支持得加 `deps` 参数，本轮不动
- 不迁 `fetchMute`：button 处直接 setMuteUntil 走 set_mute_minutes 返回值（1 次 invoke），迁到 hook 后只能调 refresh() 触发额外 get_mute_until invoke（2 次往返），延迟略增 + 无收益
- 不迁 `fetchLogs`（1000ms 日志流）：场景不同（log streaming 需要 inverse 触发模式），不属 polling-state 模板

## 验收

- `npx tsc --noEmit` ✅
- 「调试」tab → 24-bucket sparkline 60s 自动刷新行为不变；启动时显全 0 sparkline，首次 fetch 完成后切到实际数据

## 完成

- [x] PanelDebug.tsx: hourlyBuckets → usePollingState
- [x] `npx tsc --noEmit` 通过
- [x] 移到 docs/done/
