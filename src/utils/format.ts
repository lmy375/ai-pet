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

/** Pretty-print a JSON string; returns the original string if it isn't JSON. */
export function formatJson(str: string): string {
  try {
    return JSON.stringify(JSON.parse(str), null, 2);
  } catch {
    return str;
  }
}
