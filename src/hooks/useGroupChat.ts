import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { newItemId, type ChatItem, type ToolCall } from "./useChat";

/** One line in the shared group transcript (mirrors backend `GroupMessage`). */
export interface GroupMessage {
  id: string;
  speaker_kind: "human" | "agent";
  agent_id?: string;
  name: string;
  content: string;
  ts: number;
}

/** Room record for the sidebar (`group_list`). */
export interface GroupSummary {
  id: string;
  title: string;
  members: string[];
  paused: boolean;
  updated_at: number;
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
  id: string;
  title: string;
  transcript: GroupMessage[];
  members: string[];
  agents: Record<string, { items?: ChatItem[] }>;
  paused: boolean;
}

/** A `group-stream` event payload: a chat StreamEvent tagged with the room + agent. */
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
          items: [...v.items, { id: newItemId(), type: "tool", content: "", toolCalls: v.toolCalls, ts }],
          toolCalls: [],
        };
  const commitText = (v: AgentView): AgentView =>
    v.streaming.trim()
      ? { ...v, items: [...v.items, { id: newItemId(), type: "assistant", content: v.streaming, ts }], streaming: "" }
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
        items: [...v.items, { id: newItemId(), type: "assistant", content: "", images: [ev.data.dataUrl], ts }],
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
        items: [...v.items, { id: newItemId(), type: "error", content: ev.data.message, ts }],
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
  onMessage: (p: { groupId: string; message: GroupMessage }) => void;
  onInjected: (p: { groupId: string; agentId: string; items: ChatItem[] }) => void;
  onStream: (p: { groupId: string; agentId: string; event: StreamEvent }) => void;
  onAgentDone: (p: { groupId: string; agentId: string }) => void;
  onPaused: (p: { groupId: string }) => void;
  onResumed: (p: { groupId: string }) => void;
  onReset: (p: { groupId: string }) => void;
}

let liveHandlers: GroupHandlers | null = null;
let listenersStarted = false;

function ensureListeners() {
  if (listenersStarted) return;
  listenersStarted = true;
  listen<{ groupId: string; message: GroupMessage }>("group-message", (e) => liveHandlers?.onMessage(e.payload));
  listen<{ groupId: string; agentId: string; items: ChatItem[] }>("group-injected", (e) => liveHandlers?.onInjected(e.payload));
  listen<{ groupId: string; agentId: string; event: StreamEvent }>("group-stream", (e) => liveHandlers?.onStream(e.payload));
  listen<{ groupId: string; agentId: string }>("group-agent-done", (e) => liveHandlers?.onAgentDone(e.payload));
  listen<{ groupId: string }>("group-paused", (e) => liveHandlers?.onPaused(e.payload));
  listen<{ groupId: string }>("group-resumed", (e) => liveHandlers?.onResumed(e.payload));
  listen<{ groupId: string }>("group-reset", (e) => liveHandlers?.onReset(e.payload));
}

/** Drives the group-chat page: the room list, the active room's shared
 *  transcript, per-agent live sessions, membership and pause state. */
export function useGroupChat() {
  const [groups, setGroups] = useState<GroupSummary[]>([]);
  const [ready, setReady] = useState(false);
  const [activeId, setActiveId] = useState("");
  const [title, setTitle] = useState("");
  const [transcript, setTranscript] = useState<GroupMessage[]>([]);
  const [members, setMembers] = useState<string[]>([]);
  const [agents, setAgents] = useState<Record<string, AgentView>>({});
  const [paused, setPausedState] = useState(false);
  // Dedup transcript appends — group_load + the group-message event for the same
  // post can race on first mount.
  const seenMsgIds = useRef<Set<string>>(new Set());
  const activeRef = useRef(activeId);
  activeRef.current = activeId;

  const load = useCallback(async (id: string) => {
    const state = await invoke<GroupStateSnapshot>("group_load", { id });
    // The user may have switched rooms while this load was in flight.
    if (activeRef.current !== id) return;
    seenMsgIds.current = new Set(state.transcript.map((m) => m.id));
    setTitle(state.title);
    setTranscript(state.transcript);
    setMembers(state.members);
    const views: Record<string, AgentView> = {};
    for (const [agentId, st] of Object.entries(state.agents)) {
      views[agentId] = emptyView(st.items ?? []);
    }
    setAgents(views);
    setPausedState(state.paused);
  }, []);

  const loadList = useCallback(async (selectId?: string) => {
    const list = await invoke<GroupSummary[]>("group_list");
    setGroups(list);
    setActiveId((prev) => {
      const wanted = selectId ?? prev;
      if (wanted && list.some((group) => group.id === wanted)) return wanted;
      return list[0]?.id ?? "";
    });
  }, []);

  // Stable handlers (functional setState + refs only) so the module singleton can
  // hold them across renders without re-subscribing.
  const onMessage = useCallback(({ groupId, message }: { groupId: string; message: GroupMessage }) => {
    if (groupId !== activeRef.current || seenMsgIds.current.has(message.id)) return;
    seenMsgIds.current.add(message.id);
    setTranscript((prev) => [...prev, message]);
  }, []);

  const onInjected = useCallback(({ groupId, agentId, items }: { groupId: string; agentId: string; items: ChatItem[] }) => {
    if (groupId !== activeRef.current) return;
    setAgents((prev) => {
      const view = prev[agentId] ?? emptyView();
      // `group_load` may already carry items that were injected just before the
      // switch; only append the ones this view hasn't rendered yet.
      const seen = new Set(view.items.map((item) => item.id));
      const fresh = items.filter((item) => !seen.has(item.id));
      return { ...prev, [agentId]: { ...view, items: [...view.items, ...fresh], running: true } };
    });
  }, []);

  const onStream = useCallback(({ groupId, agentId, event }: { groupId: string; agentId: string; event: StreamEvent }) => {
    if (groupId !== activeRef.current) return;
    setAgents((prev) => {
      const view = prev[agentId] ?? emptyView();
      return { ...prev, [agentId]: applyStream(view, event) };
    });
  }, []);

  const onAgentDone = useCallback(({ groupId, agentId }: { groupId: string; agentId: string }) => {
    if (groupId !== activeRef.current) return;
    setAgents((prev) => {
      const view = prev[agentId];
      if (!view || !view.running) return prev;
      return { ...prev, [agentId]: { ...view, running: false } };
    });
  }, []);

  // Pausing stops all loops server-side; clear every agent's running indicator.
  const onPaused = useCallback(({ groupId }: { groupId: string }) => {
    setGroups((prev) => prev.map((group) => (group.id === groupId ? { ...group, paused: true } : group)));
    if (groupId !== activeRef.current) return;
    setPausedState(true);
    setAgents((prev) => {
      const next: Record<string, AgentView> = {};
      for (const [id, v] of Object.entries(prev)) next[id] = { ...v, running: false };
      return next;
    });
  }, []);

  const onResumed = useCallback(({ groupId }: { groupId: string }) => {
    setGroups((prev) => prev.map((group) => (group.id === groupId ? { ...group, paused: false } : group)));
    if (groupId === activeRef.current) setPausedState(false);
  }, []);

  const onReset = useCallback(({ groupId }: { groupId: string }) => {
    if (groupId !== activeRef.current) return;
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
    loadList().finally(() => setReady(true));
    return () => {
      // Only clear if we're still the live instance (avoid a remount race).
      if (liveHandlers === handlers) liveHandlers = null;
    };
  }, [onMessage, onInjected, onStream, onAgentDone, onPaused, onResumed, onReset, loadList]);

  useEffect(() => {
    if (!activeId) {
      seenMsgIds.current = new Set();
      setTitle("");
      setTranscript([]);
      setMembers([]);
      setAgents({});
      setPausedState(false);
      return;
    }
    load(activeId).catch(console.error);
  }, [activeId, load]);

  const create = useCallback(async (title: string, members: string[]): Promise<void> => {
    const state = await invoke<GroupStateSnapshot>("group_create", { title, members });
    const summary: GroupSummary = {
      id: state.id,
      title: state.title,
      members: state.members,
      paused: state.paused,
      updated_at: Date.now(),
    };
    setGroups((prev) => [summary, ...prev.filter((group) => group.id !== state.id)]);
    setActiveId(state.id);
  }, []);

  const configure = useCallback(async (id: string, title: string, members: string[]): Promise<void> => {
    const trimmed = title.trim();
    await invoke("group_configure", { id, title: trimmed, members });
    setGroups((prev) =>
      prev.map((group) => (group.id === id ? { ...group, title: trimmed, members, updated_at: Date.now() } : group)),
    );
    if (id !== activeRef.current) return;
    setTitle(trimmed);
    setMembers(members);
    setAgents((prev) => {
      const next: Record<string, AgentView> = {};
      for (const agentId of members) next[agentId] = prev[agentId] ?? emptyView();
      return next;
    });
  }, []);

  const deleteGroup = useCallback(async (id: string): Promise<void> => {
    await invoke("group_delete", { id });
    const list = await invoke<GroupSummary[]>("group_list");
    setGroups(list);
    setActiveId((prev) => (prev === id ? (list[0]?.id ?? "") : prev));
  }, []);

  const send = useCallback(async (content: string) => {
    const id = activeRef.current;
    if (id) await invoke("group_send", { id, content });
  }, []);

  const reset = useCallback(async () => {
    const id = activeRef.current;
    if (id) await invoke("group_reset", { id });
  }, []);

  const setPaused = useCallback(async (next: boolean) => {
    const id = activeRef.current;
    if (!id) return;
    // Optimistic — the group-paused/resumed event reconciles + clears indicators.
    setPausedState(next);
    setGroups((prev) => prev.map((group) => (group.id === id ? { ...group, paused: next } : group)));
    await invoke("group_set_paused", { id, paused: next });
  }, []);

  const anyRunning = Object.values(agents).some((v) => v.running);

  return {
    groups,
    ready,
    activeId,
    setActiveId,
    title,
    transcript,
    members,
    agents,
    paused,
    anyRunning,
    create,
    configure,
    deleteGroup,
    send,
    reset,
    setPaused,
  };
}
