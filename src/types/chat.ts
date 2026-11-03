import type { StreamEvent, StreamView, Usage } from "../hooks/streamView";

/** Sentinel title for a not-yet-named session. Stored verbatim on disk (so old
 *  sessions keep matching); translate it at display time, never compare against
 *  a localized string. */
export const DEFAULT_SESSION_TITLE = "新会话";

export interface ToolCall {
  name: string;
  arguments: string;
  result?: string;
  /** Live view only: persisted tool calls have always finished, so the backend
   *  no longer stores this flag. */
  isRunning?: boolean;
}

export interface ChatItem {
  id: string; // stable per-item id for React keys + multi-select; stamped by whichever side creates the item
  type: "user" | "assistant" | "tool" | "error" | "notification" | "group";
  content: string;
  reasoning?: string; // assistant items: chain-of-thought from a reasoning model, shown in a collapsed block. Display-only — never sent back to the model.
  images?: string[]; // base64 data URLs rendered in the bubble — user pastes, or tool-produced images (e.g. screenshots) on assistant items
  toolCalls?: ToolCall[];
  ts: number; // epoch ms, stamped at creation
  label?: string; // notification items: the finished task's label (localized into the line at render time)
  detail?: string; // notification items: the task's full result, shown on expand
  // Group items: a line injected into a group agent's private context. The
  // sender is carried as data (never parsed out of `content`), so ChatThread can
  // render it as an incoming chat message instead of raw tag text.
  senderKind?: "human" | "agent";
  senderId?: string;
  senderName?: string;
}

export interface SessionMeta {
  id: string;
  title: string;
  created_at: string;
  updated_at: string;
}

export interface SessionIndex {
  active_id: string;
  sessions: SessionMeta[];
}

/** A session as the backend renders it for a window: the display transcript
 *  only. The LLM conversation stays in the backend and is never sent here. */
export interface SessionView {
  id: string;
  title: string;
  created_at: string;
  updated_at: string;
  items: ChatItem[];
  context_usage?: Usage | null;
}

/** Turn activity from the backend (`turn` event, camelCase via serde). One
 *  stream for every session; this hook keeps what concerns its own. */
export type TurnOrigin =
  | { type: "user"; text: string }
  | { type: "completion"; label: string };

export type TurnNotice =
  | { kind: "started"; sessionId: string; turnId: string; origin: TurnOrigin; startedAt: number }
  | { kind: "stream"; sessionId: string; turnId: string; seq: number; event: StreamEvent }
  | { kind: "finished"; sessionId: string; turnId: string };

/** Everything a running turn has produced (`attach_turn`): the backend's own
 *  committed items, the still-streaming tail, and the turn's opening origin. */
export interface TurnSnapshot {
  turnId: string;
  origin: TurnOrigin;
  startedAt: number;
  seq: number;
  items: ChatItem[];
  response: string;
  reasoning: string;
  toolCalls: ToolCall[];
  usage: Usage | null;
  spent: number;
}

/** The turn this window is following: id, last applied `seq`, and its view. */
export interface TurnState {
  id: string;
  /** Epoch ms the backend stamped on the turn — never `Date.now()` here, so a
   *  window that re-attaches mid-turn shows the real elapsed time, not zero. */
  startedAt: number;
  seq: number;
  view: StreamView;
}