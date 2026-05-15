import { useCallback, useState } from "react";

/// 搜索框输入历史 —— localStorage 持久化的 ring buffer。
///
/// 通用 caller：PanelMemory（memory_search 成功后 push）、PanelTasks（搜索框
/// Enter push）等。每条 query 通过 `push` 入栈：
/// - trim 后空字符串 → noop
/// - 同 query 已在栈中 → 移到栈顶（dedup move-to-front）
/// - 超过 `cap` 条 → 尾部丢弃
/// - 同步写 localStorage；写盘失败（配额满 / 隐私窗口）静默
///
/// JSON 损坏 / localStorage 不可用 / 非 string 项 → 兜底为空数组，不阻塞功能。
export function useSearchHistory(
  storageKey: string,
  cap = 5,
): { history: string[]; push: (kw: string) => void } {
  const [history, setHistory] = useState<string[]>(() => {
    try {
      const raw = window.localStorage.getItem(storageKey);
      if (!raw) return [];
      const arr = JSON.parse(raw);
      if (Array.isArray(arr)) {
        return arr
          .filter((v): v is string => typeof v === "string")
          .slice(0, cap);
      }
    } catch {
      // ignore
    }
    return [];
  });
  const push = useCallback(
    (kw: string) => {
      const trimmed = kw.trim();
      if (!trimmed) return;
      setHistory((prev) => {
        const next = [trimmed, ...prev.filter((x) => x !== trimmed)].slice(0, cap);
        try {
          window.localStorage.setItem(storageKey, JSON.stringify(next));
        } catch {
          // ignore：内存里 next 仍生效，下次启动从 localStorage 上次成功写入处恢复
        }
        return next;
      });
    },
    [storageKey, cap],
  );
  return { history, push };
}
