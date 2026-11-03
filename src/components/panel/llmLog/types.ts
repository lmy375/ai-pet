/** One line of LLM log; the dashboard groups rows by kind. */
export type LogKind = "chat" | "sub" | "group" | "heartbeat" | "swarm";

export interface RoundStat {
  round: number;
  ttft_ms: number | null;
  total_ms: number;
  tools: string[];
}

/** One line of `index.jsonl` — everything a collapsed row needs, ~300 bytes. */
export interface LlmMeta {
  id: string;
  kind: LogKind;
  /** Agent id for a group run, parent session for a sub-agent, else empty. */
  label: string;
  model: string;
  request_time: string;
  rounds: RoundStat[];
  preview: string;
}

/** The body file, fetched only when a row is opened. Metadata lives only in
 * the index entry above — the body has no second copy. */
export interface LlmEntry {
  first_token_time: string | null;
  done_time: string;
  messages: Array<{ role: string; content: unknown }>;
  response: {
    text: string;
    reasoning?: string | null;
    tool_calls: ToolCall[];
  };
}

export type ToolCall = { call_id?: string; fn_name?: string; fn_arguments?: unknown };

// Display order, most useful first. Retention is per kind too (sub shares the
// chat bucket), so a noisy heartbeat run can never push chat logs off the list —
// but it can still fill a hundred rows, hence the collapsed-by-default groups.
export const KINDS: LogKind[] = ["chat", "sub", "group", "heartbeat", "swarm"];