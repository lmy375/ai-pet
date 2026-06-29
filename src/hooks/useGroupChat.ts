import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { ChatItem, ToolCall } from "./useChat";

/** One line in the shared group transcript (mirrors backend `GroupMessage`). */
export interface GroupMessage {
  id: string;
  speaker_kind: "human" | "agent";
  agent_id?: string;
  name: string;
  content: string;
  ts: number;
}

/** Live + committed display state for one agent's session (Agent sub-tab). */
export interface AgentView {
  items: ChatItem[];
  toolCalls: ToolCall[]; // in-progress (not yet flushed) tool calls
  streaming: string; // in-progress assistant text
  running: boolean;
}

/** Backend `GroupState` snapshot from `group_load`. */
interface GroupStateSnapshot {
  transcript: GroupMessage[];
  members: string[];
  agents: Record<string, { items?: ChatItem[] }>;
  paused: boolean;
}

/** A `group-stream` event payload: a chat StreamEvent tagged with the agent. */
type StreamEvent =
  | { event: "chunk"; data: { text: string } }
  | { event: "toolStart"; data: { name: string; arguments: string } }
  | { event: "toolResult"; data: { name: string; result: string } }
  | { event: "image"; data: { dataUrl: string } }
  | { event: "usage"; data: Record<string, number> }
  | { event: "done"; data: Record<string, never> }
  | { event: "error"; data: { message: string } };

function emptyView(items: ChatItem[] = []): AgentView {
  return { items, toolCalls: [], streaming: "", running: false };
}

/** Apply one StreamEvent to an agent's view — the exact reducer `useChat` runs,
 *  per-agent and without session persistence (the backend persists items). */
function applyStream(view: AgentView, ev: StreamEvent): AgentView {
  const ts = Date.now();
  const flushTools = (v: AgentView): AgentView =>
    v.toolCalls.length === 0
      ? v
      : {
          ...v,
          items: [...v.items, { type: "tool", content: "", toolCalls: v.toolCalls, ts }],
          toolCalls: [],
        };
  const commitText = (v: AgentView): AgentView =>
    v.streaming.trim()
      ? { ...v, items: [...v.items, { type: "assistant", content: v.streaming, ts }], streaming: "" }
      : { ...v, streaming: "" };

  switch (ev.event) {
    case "chunk": {
      const v = flushTools(view);
      return { ...v, running: true, streaming: v.streaming + ev.data.text };
    }
    case "toolStart": {
      const v = commitText(view);
      return {
        ...v,
        running: true,
        toolCalls: [...v.toolCalls, { name: ev.data.name, arguments: ev.data.arguments, isRunning: true }],
      };
    }
    case "toolResult": {
      let matched = false;
      const toolCalls = view.toolCalls.map((tc) => {
        if (!matched && tc.name === ev.data.name && tc.isRunning) {
          matched = true;
          return { ...tc, result: ev.data.result, isRunning: false };
        }
        return tc;
      });
      return { ...view, toolCalls };
    }
    case "image": {
      const v = flushTools(view);
      return {
        ...v,
        items: [...v.items, { type: "assistant", content: "", images: [ev.data.dataUrl], ts }],
      };
    }
    case "done": {
      const v = commitText(flushTools(view));
      return { ...v, running: false };
    }
    case "error": {
      const v = flushTools(view);
      return {
        ...v,
        items: [...v.items, { type: "error", content: ev.data.message, ts }],
        streaming: "",
        running: false,
      };
    }
    default:
      return view;
  }
}

// --- Single, app-lifetime listener registration -----------------------------
//
// `listen()` resolves async, so registering it inside a React effect leaks a
// second listener under StrictMode (mount→unmount→remount) and Vite HMR — every
// event then fires twice, which interleaves duplicate stream chunks and doubles
// items. Streaming events have no per-event id to dedup on, so instead we
// register each Tauri listener exactly ONCE at module scope (the same pattern
// `i18n/index.ts` uses) and dispatch to whichever hook instance is live. One
// listener → one delivery, no matter how many times the component remounts.

interface GroupHandlers {
  onMessage: (m: GroupMessage) => void;
  onInjected: (p: { agentId: string; items: ChatItem[] }) => void;
  onStream: (p: { agentId: string; event: StreamEvent }) => void;
  onAgentDone: (p: { agentId: string }) => void;
  onPaused: () => void;
  onResumed: () => void;
  onReset: () => void;
}

let liveHandlers: GroupHandlers | null = null;
let listenersStarted = false;

function ensureListeners() {
  if (listenersStarted) return;
  listenersStarted = true;
  listen<GroupMessage>("group-message", (e) => liveHandlers?.onMessage(e.payload));
  listen<{ agentId: string; items: ChatItem[] }>("group-injected", (e) => liveHandlers?.onInjected(e.payload));
  listen<{ agentId: string; event: StreamEvent }>("group-stream", (e) => liveHandlers?.onStream(e.payload));
  listen<{ agentId: string }>("group-agent-done", (e) => liveHandlers?.onAgentDone(e.payload));
  listen("group-paused", () => liveHandlers?.onPaused());
  listen("group-resumed", () => liveHandlers?.onResumed());
  listen("group-reset", () => liveHandlers?.onReset());
}

/** Drives the group-chat page: the shared transcript, per-agent live sessions,
 *  membership, and the budget-reached flag. */
export function useGroupChat() {
  const [transcript, setTranscript] = useState<GroupMessage[]>([]);
  const [members, setMembers] = useState<string[]>([]);
  const [agents, setAgents] = useState<Record<string, AgentView>>({});
  const [paused, setPausedState] = useState(false);
  // Dedup transcript appends — group_load + the group-message event for the same
  // post can race on first mount.
  const seenMsgIds = useRef<Set<string>>(new Set());

  const load = useCallback(async () => {
    const state = await invoke<GroupStateSnapshot>("group_load");
    seenMsgIds.current = new Set(state.transcript.map((m) => m.id));
    setTranscript(state.transcript);
    setMembers(state.members);
    const views: Record<string, AgentView> = {};
    for (const [id, st] of Object.entries(state.agents)) {
      views[id] = emptyView(st.items ?? []);
    }
    setAgents(views);
    setPausedState(state.paused);
  }, []);

  // Stable handlers (functional setState + refs only) so the module singleton can
  // hold them across renders without re-subscribing.
  const onMessage = useCallback((msg: GroupMessage) => {
    if (seenMsgIds.current.has(msg.id)) return;
    seenMsgIds.current.add(msg.id);
    setTranscript((prev) => [...prev, msg]);
  }, []);

  const onInjected = useCallback(({ agentId, items }: { agentId: string; items: ChatItem[] }) => {
    setAgents((prev) => {
      const view = prev[agentId] ?? emptyView();
      return { ...prev, [agentId]: { ...view, items: [...view.items, ...items], running: true } };
    });
  }, []);

  const onStream = useCallback(({ agentId, event }: { agentId: string; event: StreamEvent }) => {
    setAgents((prev) => {
      const view = prev[agentId] ?? emptyView();
      return { ...prev, [agentId]: applyStream(view, event) };
    });
  }, []);

  const onAgentDone = useCallback(({ agentId }: { agentId: string }) => {
    setAgents((prev) => {
      const view = prev[agentId];
      if (!view || !view.running) return prev;
      return { ...prev, [agentId]: { ...view, running: false } };
    });
  }, []);

  // Pausing stops all loops server-side; clear every agent's running indicator.
  const onPaused = useCallback(() => {
    setPausedState(true);
    setAgents((prev) => {
      const next: Record<string, AgentView> = {};
      for (const [id, v] of Object.entries(prev)) next[id] = { ...v, running: false };
      return next;
    });
  }, []);

  const onResumed = useCallback(() => setPausedState(false), []);

  const onReset = useCallback(() => {
    seenMsgIds.current = new Set();
    setTranscript([]);
    setAgents((prev) => {
      const next: Record<string, AgentView> = {};
      for (const id of Object.keys(prev)) next[id] = emptyView();
      return next;
    });
    setPausedState(false);
  }, []);

  useEffect(() => {
    ensureListeners();
    const handlers: GroupHandlers = { onMessage, onInjected, onStream, onAgentDone, onPaused, onResumed, onReset };
    liveHandlers = handlers;
    load();
    return () => {
      // Only clear if we're still the live instance (avoid a remount race).
      if (liveHandlers === handlers) liveHandlers = null;
    };
  }, [onMessage, onInjected, onStream, onAgentDone, onPaused, onResumed, onReset, load]);

  const send = useCallback(async (content: string) => {
    await invoke("group_send", { content });
  }, []);

  const updateMembers = useCallback(async (ids: string[]) => {
    await invoke("group_set_members", { ids });
    setMembers(ids);
    setAgents((prev) => {
      const next: Record<string, AgentView> = {};
      for (const id of ids) next[id] = prev[id] ?? emptyView();
      return next;
    });
  }, []);

  const reset = useCallback(async () => {
    await invoke("group_reset");
  }, []);

  const setPaused = useCallback(async (next: boolean) => {
    // Optimistic — the group-paused/resumed event reconciles + clears indicators.
    setPausedState(next);
    await invoke("group_set_paused", { paused: next });
  }, []);

  const anyRunning = Object.values(agents).some((v) => v.running);

  return { transcript, members, agents, paused, anyRunning, send, updateMembers, reset, setPaused };
}
