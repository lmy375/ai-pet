/** Shared formatting helpers used across chat and log views. */
import type { TKey } from "../i18n";

const MINUTE = 60 * 1000;
const HOUR = 60 * MINUTE;

type Translate = (key: TKey, vars?: Record<string, string | number>) => string;

/** Epoch ms → how long ago, as a message shows it: `刚刚` / `7 分钟前` /
 *  `3 小时前` today, `昨天 14:30`, then `9/15 14:30`.
 *
 *  Computed at render time with no ticker: the thread re-renders on every turn
 *  event and on window focus, so a label can only drift while the window sits
 *  idle and untouched. */
export function formatWhen(ts: number, t: Translate): string {
  const diff = Date.now() - ts;
  if (diff < MINUTE) return t("time.justNow");
  if (diff < HOUR) return t("time.minutesAgo", { n: Math.floor(diff / MINUTE) });
  const midnight = new Date();
  midnight.setHours(0, 0, 0, 0);
  if (ts >= midnight.getTime()) return t("time.hoursAgo", { n: Math.floor(diff / HOUR) });
  const d = new Date(ts);
  if (ts >= midnight.getTime() - 24 * HOUR) return t("time.yesterday", { time: formatHm(ts) });
  return `${d.getMonth() + 1}/${d.getDate()} ${formatHm(ts)}`;
}

/** Epoch ms → `HH:MM` (used for chat timestamps). */
export function formatHm(ts: number): string {
  const d = new Date(ts);
  const hh = d.getHours().toString().padStart(2, "0");
  const mm = d.getMinutes().toString().padStart(2, "0");
  return `${hh}:${mm}`;
}

/** ISO string → `HH:MM:SS` (used for LLM log entries); `—` when missing. */
export function formatIsoTime(ts: string | undefined | null): string {
  if (!ts) return "—";
  const t = ts.split("T")[1];
  return t ? t.slice(0, 8) : ts;
}

/** Parse a JSON-ish payload: a JSON string becomes its value, anything else
 *  (plain text, an already-parsed object) is returned untouched. Tool payloads
 *  arrive both ways — the chat stream carries raw JSON strings, the LLM log
 *  carries genai's already-parsed `fn_arguments`. */
export function parseJsonish(value: unknown): unknown {
  if (typeof value !== "string") return value;
  const trimmed = value.trim();
  if (!trimmed.startsWith("{") && !trimmed.startsWith("[")) return value;
  try {
    return JSON.parse(trimmed);
  } catch {
    return value; // partial JSON while a tool call is still streaming
  }
}
