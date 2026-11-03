import type { ChatItem, ToolCall } from "../types/chat";

/** One event from an agent run (mirrors the backend's `StreamEvent`, camelCase
 *  via serde). The same shape arrives from chat turns, group agents and swarm
 *  main/worker streams. */
export type StreamEvent =
  | { event: "chunk"; data: { text: string } }
  | { event: "reasoning"; data: { text: string } }
  | { event: "toolStart"; data: { name: string; arguments: string } }
  | { event: "toolResult"; data: { name: string; result: string } }
  | { event: "image"; data: { dataUrl: string } }
  | { event: "usage"; data: { totalTokens: number; contextWindow: number } }
  | { event: "done"; data: Record<string, never> }
  | { event: "error"; data: { message: string } };

export type Usage = { used: number; total: number };

/**
 * What an agent run has produced so far: items already final within the run,
 * plus the text / thinking / tool calls still streaming. Every view (chat,
 * group agent, swarm agent) uses this shape; the backend's snapshot produces it
 * directly for chat turns.
 */
export interface StreamView {
  items: ChatItem[];
  streaming: string;
  reasoning: string;
  toolCalls: ToolCall[];
  usage: Usage | null;
  /** Tokens this run has burned so far: every round's own total added up (a
   *  tool loop re-sends the conversation each round, so that IS the cost).
   *  Distinct from `usage`, which is the latest round's context occupancy. */
  spent: number;
  running: boolean;
}

export function emptyView(items: ChatItem[] = []): StreamView {
  return { items, streaming: "", reasoning: "", toolCalls: [], usage: null, spent: 0, running: false };
}

// Stable, collision-free id for a chat item an interface creates before the
// backend's copy arrives (the optimistic user bubble, the in-progress run's
// items). Never key/select by array index.
let chatItemSeq = 0;
export function newItemId(): string {
  chatItemSeq += 1;
  return `it-${Date.now().toString(36)}-${chatItemSeq.toString(36)}`;
}

function flushToolCalls(v: StreamView): StreamView {
  if (v.toolCalls.length === 0) return v;
  return {
    ...v,
    items: [...v.items, { id: newItemId(), type: "tool", content: "", toolCalls: v.toolCalls, ts: Date.now() }],
    toolCalls: [],
  };
}

/** Commit the streaming text + thinking as one assistant item (none if both empty). */
function commitText(v: StreamView): StreamView {
  if (!v.streaming.trim() && !v.reasoning.trim()) return { ...v, streaming: "", reasoning: "" };
  return {
    ...v,
    items: [
      ...v.items,
      {
        id: newItemId(),
        type: "assistant",
        content: v.streaming,
        reasoning: v.reasoning || undefined,
        ts: Date.now(),
      },
    ],
    streaming: "",
    reasoning: "",
  };
}

/**
 * Apply one stream event to a view. This is the single fold shared by the chat
 * window, group agent tabs and swarm views — mirrors the backend's
 * `ItemBuilder`, so the live view and a reloaded transcript agree.
 */
export function applyStream(view: StreamView, e: StreamEvent): StreamView {
  switch (e.event) {
    case "chunk": {
      const v = flushToolCalls(view);
      return { ...v, streaming: v.streaming + e.data.text, running: true };
    }
    case "reasoning": {
      // A new round's thinking starts only after the previous round's tool
      // calls have all returned, so this is the round boundary: close their
      // group here. Without it a reasoning-only agent keeps every round's calls
      // in one buffer and flushes them as one item after all the thinking.
      const v = flushToolCalls(view);
      return { ...v, reasoning: v.reasoning + e.data.text, running: true };
    }
    case "toolStart": {
      // Text/thinking streamed before the call stays with the round it came from.
      const v = commitText(view);
      return {
        ...v,
        toolCalls: [...v.toolCalls, { name: e.data.name, arguments: e.data.arguments, isRunning: true }],
        running: true,
      };
    }
    case "toolResult": {
      // Results arrive in call order; attach to the FIRST still-running call of
      // that name so two parallel same-named calls don't both take the first result.
      let matched = false;
      return {
        ...view,
        toolCalls: view.toolCalls.map((tc) => {
          if (!matched && tc.name === e.data.name && tc.isRunning) {
            matched = true;
            return { ...tc, result: e.data.result, isRunning: false };
          }
          return tc;
        }),
      };
    }
    case "image": {
      const v = flushToolCalls(view);
      return {
        ...v,
        items: [...v.items, { id: newItemId(), type: "assistant", content: "", images: [e.data.dataUrl], ts: Date.now() }],
      };
    }
    case "usage":
      return {
        ...view,
        usage: { used: e.data.totalTokens, total: e.data.contextWindow },
        spent: view.spent + e.data.totalTokens,
      };
    case "done":
      return { ...commitText(flushToolCalls(view)), running: false };
    case "error": {
      const v = flushToolCalls(view);
      return {
        ...v,
        streaming: "",
        reasoning: "",
        items: [...v.items, { id: newItemId(), type: "error", content: e.data.message, ts: Date.now() }],
        running: false,
      };
    }
  }
}