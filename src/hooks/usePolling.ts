import { useEffect, useRef } from "react";

/**
 * Call `fn` once immediately, then every `intervalMs`, cleaning up on unmount.
 * The latest `fn` is always used (kept in a ref), so callers don't need to
 * memoize it and the interval only resets when `intervalMs` changes — no churn
 * when the callback closes over changing state.
 */
export function usePolling(fn: () => void, intervalMs: number) {
  const saved = useRef(fn);
  useEffect(() => {
    saved.current = fn;
  });
  useEffect(() => {
    const tick = () => saved.current();
    tick();
    const id = setInterval(tick, intervalMs);
    return () => clearInterval(id);
  }, [intervalMs]);
}
