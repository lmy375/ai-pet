// 跨窗口共享的"用户发过的消息" 历史栈（shell-readline 风 ↑/↓ 召回）。
//
// 两个使用方：
// - `ChatPanel.tsx`（桌面 pet 窗底部输入框）
// - `panel/PanelChat.tsx`（面板里的大聊天框）
//
// 用同一个 localStorage key `pet-chat-history`，让"我刚才说过什么"在 pet 窗 / 大
// 面板任一边按 `↑` 都能召回，跨 mount / 跨重启也保留。
//
// 约定：newest-at-front（index 0 是最新）。配合 dedup + move-to-front + cap 20。

export const SENT_HISTORY_CAP = 20;
const HISTORY_KEY = "pet-chat-history";

/// localStorage 取出 + 类型守门（每项必须是非空 string）。invalid JSON / 缺
/// 失项 / 隐私窗口 → 空数组（功能性 fallback：仅丧失 ↑/↓ 召回，不阻塞发送）。
export function readSentHistory(): string[] {
  try {
    const raw = localStorage.getItem(HISTORY_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw) as unknown;
    if (Array.isArray(parsed)) {
      return parsed
        .filter((v): v is string => typeof v === "string" && v.length > 0)
        .slice(0, SENT_HISTORY_CAP);
    }
  } catch {
    // localStorage 不可用 / JSON 损坏 — 静默退化为空历史
  }
  return [];
}

/// 写盘。配额满 / 隐私模式 → 静默失败（本次发送照常）。
function writeSentHistory(list: string[]): void {
  try {
    localStorage.setItem(HISTORY_KEY, JSON.stringify(list));
  } catch {
    // ignore — 见 readSentHistory 的容错策略
  }
}

/// push 新消息到历史栈：trim → dedup（同内容旧位置移除）→ unshift → cap
/// → 写盘 → 返回新数组。调用方 setState(returned-value) 即可。
///
/// trimmed 为空字符串时 noop（返回当前历史）—— 防御 caller 没自己 trim。
export function pushSentHistory(text: string): string[] {
  const trimmed = text.trim();
  if (trimmed.length === 0) return readSentHistory();
  const cur = readSentHistory();
  const next = [trimmed, ...cur.filter((x) => x !== trimmed)].slice(0, SENT_HISTORY_CAP);
  writeSentHistory(next);
  return next;
}
