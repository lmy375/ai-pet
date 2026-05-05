import { useCallback, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

// 桌面气泡的历史导航：让用户在 bubble 上原地用 ◀/▶ 翻看最近 N 条 proactive
// 发言（speech_history.log 由后端维护，已有 `get_recent_speeches` Tauri 命令）。
//
// 状态机：
// - `index === null` → live 模式，bubble 显示外部传入的 displayMessage
// - `index === i (0..N-1)` → history 模式，bubble 显示 `speeches[i]`（已 strip ts）
//
// `i = 0` 是最新一条 proactive（栈顶），`i = N-1` 是最久。`enterPrev` 把 index 往
// 大走（往更早翻），`next` 往 0 走，到 -1 时回 live（null）。
//
// 加载策略：首次 `enterPrev` 才 invoke 后端拉历史；之后翻动靠本地数组。新 proactive
// 到来时（外部调 `reset()`）会清掉缓存，下次进入历史时重新加载，保证看到最新攒下
// 的几条。

const HISTORY_LIMIT = 10;

export function useBubbleHistory() {
  const [speeches, setSpeeches] = useState<string[] | null>(null);
  const [index, setIndex] = useState<number | null>(null);
  const [loading, setLoading] = useState(false);

  const ensureLoaded = useCallback(async (): Promise<string[]> => {
    if (speeches !== null) return speeches;
    setLoading(true);
    try {
      const raw = await invoke<string[]>("get_recent_speeches", { n: HISTORY_LIMIT });
      // 后端返回顺序是"oldest of kept window first, newest last"（见
      // speech_history::parse_recent 注释）。本 hook 想要"newest first"以匹配
      // i=0 是最新的语义，所以这里反转一次。
      const stripped = raw.map(stripTimestamp).filter((s) => s.length > 0).reverse();
      setSpeeches(stripped);
      return stripped;
    } catch (e) {
      console.error("Failed to load speech history for bubble nav:", e);
      setSpeeches([]);
      return [];
    } finally {
      setLoading(false);
    }
  }, [speeches]);

  const enterPrev = useCallback(async () => {
    const list = await ensureLoaded();
    if (list.length === 0) return;
    setIndex((prev) => {
      // live → 进入历史从 i=0 起步
      if (prev === null) return 0;
      // 已在历史里 → 往更早翻一格，封顶到末尾
      return Math.min(prev + 1, list.length - 1);
    });
  }, [ensureLoaded]);

  const next = useCallback(() => {
    setIndex((prev) => {
      if (prev === null || prev <= 0) return null; // 回 live
      return prev - 1;
    });
  }, []);

  const reset = useCallback(() => {
    setIndex(null);
    setSpeeches(null); // 让下次进入历史重新拉，吸收新到的 proactive
  }, []);

  const displayed: string | null =
    index !== null && speeches && index >= 0 && index < speeches.length
      ? speeches[index]
      : null;

  const total = speeches?.length ?? 0;
  const indicator: string | null =
    index !== null && total > 0 ? `${index + 1}/${total}` : null;

  // ◀ 可点的状态机：
  // - 历史尚未加载（speeches === null）→ 允许点（第一次点击负责触发加载）
  // - 已加载且空 → 禁用（让按钮自然褪色，避免误导）
  // - 已加载非空 + live 模式 → 允许
  // - 已加载非空 + history 模式 → 仅在未到末尾时允许
  const canPrev =
    speeches === null
      ? true
      : speeches.length === 0
        ? false
        : index === null
          ? true
          : index < speeches.length - 1;
  // ▶ 可点：history 模式下任意位置都能（i=0 时点 ▶ 回 live）。live 模式下隐藏。
  const canNext = index !== null;

  return {
    displayed,
    indicator,
    canPrev,
    canNext,
    enterPrev,
    next,
    reset,
    isHistoryMode: index !== null,
    loading,
  };
}

/// 后端 speech_history.log 的行格式是 `<ISO ts> <text>`。前端展示要的是 text
/// 部分；与 Rust 侧 `speech_history::strip_timestamp` 等价的极简实现。
/// 没有空格的行原样返回（与 Rust 实现的 fallback 对齐，避免边界数据丢内容）。
function stripTimestamp(line: string): string {
  const idx = line.indexOf(" ");
  if (idx < 0) return line;
  return line.slice(idx + 1);
}
