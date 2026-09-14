/** Shared formatting helpers used across chat and log views. */

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
