import { useCallback, useEffect, useRef, useState } from "react";

/// 共享"周期轮询 + 手动刷新 + refreshing flag" 三件套。
///
/// 行为：
/// - 挂载时立即 fetch 一次
/// - 每 `intervalMs` 自动 fetch
/// - 调用方调 `refresh()` 立即触发；in-flight 期间重复点 ignore（refreshing flag 守门）
/// - fetcher throw 时静默吞掉错误，保持上一份 data（"不闪 null"）—— 调用方
///   想细分错误处理时自己在 fetcher 里 catch
///
/// 签名变体：
/// - `usePollingState(fetcher, intervalMs)` → data: T | null
/// - `usePollingState(fetcher, intervalMs, initial)` → data: T
/// - 第 4 个 `deps` 参数（任一签名都可加）：deps 元素变化时除周期外额外立
///   即 refetch + 重启 interval（同 useEffect deps 语义）。caller 用来跟随
///   "切换分页 / 切换天数 / 切换 chat id" 等 state 变化重 fetch。
///
/// 调用方：PanelDebug 两条 strip（无 initial）、App.tsx pet 窗 pill（initial =
/// {overdue: 0, done_today: 0}）、speechWindowCount（带 [speechWindowDays] deps）。
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
  initial: T,
  deps: ReadonlyArray<unknown>,
): { data: T; refresh: () => Promise<void>; refreshing: boolean };
export function usePollingState<T>(
  fetcher: () => Promise<T>,
  intervalMs: number,
  initial: undefined,
  deps: ReadonlyArray<unknown>,
): { data: T | null; refresh: () => Promise<void>; refreshing: boolean };
export function usePollingState<T>(
  fetcher: () => Promise<T>,
  intervalMs: number,
  initial?: T,
  deps?: ReadonlyArray<unknown>,
): {
  data: T | null;
  refresh: () => Promise<void>;
  refreshing: boolean;
} {
  const [data, setData] = useState<T | null>(initial ?? null);
  const [refreshing, setRefreshing] = useState(false);
  // fetcher 通过 ref 保持最新 —— 避免调用方传 inline lambda 导致 useEffect
  // 每次 re-subscribe（30s 轮询会抖）。
  const fetcherRef = useRef(fetcher);
  fetcherRef.current = fetcher;
  // in-flight ref：同步守门 dedupe 快速重复点。setRefreshing 是异步 setState
  // 不能立即读到新值；ref 才能 atomic check + set。
  const inFlight = useRef(false);

  const refresh = useCallback(async () => {
    if (inFlight.current) return;
    inFlight.current = true;
    setRefreshing(true);
    try {
      const next = await fetcherRef.current();
      setData(next);
    } catch {
      // 静默：保持上一份 data，避免周期 fetch 偶发失败闪 null
    } finally {
      setRefreshing(false);
      inFlight.current = false;
    }
  }, []);

  useEffect(() => {
    void refresh();
    const id = window.setInterval(() => void refresh(), intervalMs);
    return () => window.clearInterval(id);
    // `...deps` 让 caller 控制 dep 变化时重新立即 refetch + 重启 interval。
    // ESLint react-hooks/exhaustive-deps 看不穿 spread 的稳定性，需要 disable。
    // caller 跨 render 应传同一形状的 deps 数组（rules-of-hooks 前提）。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [intervalMs, refresh, ...(deps ?? [])]);

  return { data, refresh, refreshing };
}
