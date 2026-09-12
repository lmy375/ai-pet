import { useState, useRef, useEffect, useCallback, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useTauriEvent } from "./useTauriEvent";

/** Sentinel title for a not-yet-named session. Stored verbatim on disk (so old
 *  sessions keep matching); translate it at display time, never compare against
 *  a localized string. */
export const DEFAULT_SESSION_TITLE = "新会话";

export interface ToolCall {
  name: string;
  arguments: string;
  result?: string;
  isRunning: boolean;
}

// Stable, collision-free id for a chat item this window creates before the
// backend's copy arrives (the optimistic user bubble, the in-progress turn's
// items). Never key/select by array index — it shifts when items are inserted
// or removed and ends up deleting the wrong rows.
let chatItemSeq = 0;
export function newItemId(): string {
  chatItemSeq += 1;
  return `it-${Date.now().toString(36)}-${chatItemSeq.toString(36)}`;
}

export interface ChatItem {
  id: string; // stable per-item id for React keys + multi-select; stamped by whichever side creates the item
  type: "user" | "assistant" | "tool" | "error" | "notification";
  content: string;
  reasoning?: string; // assistant items: chain-of-thought from a reasoning model, shown in a collapsed block. Display-only — never sent back to the model.
  images?: string[]; // base64 data URLs rendered in the bubble — user pastes, or tool-produced images (e.g. screenshots) on assistant items
  toolCalls?: ToolCall[];
  ts: number; // epoch ms, stamped at creation
  label?: string; // notification items: the finished task's label (localized into the line at render time)
  detail?: string; // notification items: the task's full result, shown on expand
}

export interface SessionMeta {
  id: string;
  title: string;
  created_at: string;
  updated_at: string;
}

interface SessionIndex {
  active_id: string;
  sessions: SessionMeta[];
}

type Usage = { used: number; total: number };

/** A session as the backend renders it for a window: the display transcript
 *  only. The LLM conversation stays in the backend and is never sent here. */
interface SessionView {
  id: string;
  title: string;
  created_at: string;
  updated_at: string;
  items: ChatItem[];
  context_usage?: Usage | null;
}

type StreamEvent =
  | { event: "chunk"; data: { text: string } }
  | { event: "reasoning"; data: { text: string } }
  | { event: "toolStart"; data: { name: string; arguments: string } }
  | { event: "toolResult"; data: { name: string; result: string } }
  | { event: "image"; data: { dataUrl: string } }
  | { event: "usage"; data: { promptTokens: number; totalTokens: number; contextWindow: number } }
  | { event: "done"; data: Record<string, never> }
  | { event: "error"; data: { message: string } };

/** Turn activity from the backend (`turn` event, camelCase via serde). One
 *  stream for every session; this hook keeps what concerns its own. */
type TurnNotice =
  | { kind: "started"; sessionId: string; turnId: string }
  | { kind: "stream"; sessionId: string; turnId: string; seq: number; event: StreamEvent }
  | { kind: "finished"; sessionId: string; turnId: string };

type StreamNotice = Extract<TurnNotice, { kind: "stream" }>;

/** Everything a running turn has streamed so far (`attach_turn`). */
interface TurnSnapshot {
  turnId: string;
  seq: number;
  events: StreamEvent[];
}

/**
 * What the turn in progress has produced so far, folded from its stream
 * events: items already final within the turn (text before a tool call, tool
 * rows, images), plus the text / thinking / tool calls still streaming. Pure
 * data, so a late joiner replays the backend's snapshot through the same
 * reducer and lands on the identical view.
 */
interface TurnView {
  items: ChatItem[];
  response: string;
  reasoning: string;
  toolCalls: ToolCall[];
  usage: Usage | null;
}

const EMPTY_VIEW: TurnView = { items: [], response: "", reasoning: "", toolCalls: [], usage: null };

function flushToolCalls(v: TurnView): TurnView {
  if (v.toolCalls.length === 0) return v;
  return {
    ...v,
    items: [...v.items, { id: newItemId(), type: "tool", content: "", toolCalls: v.toolCalls, ts: Date.now() }],
    toolCalls: [],
  };
}

/** Commit the streaming text + thinking as one assistant item (none if both empty). */
function commitText(v: TurnView): TurnView {
  if (!v.response.trim() && !v.reasoning.trim()) return { ...v, response: "", reasoning: "" };
  return {
    ...v,
    items: [
      ...v.items,
      { id: newItemId(), type: "assistant", content: v.response, reasoning: v.reasoning || undefined, ts: Date.now() },
    ],
    response: "",
    reasoning: "",
  };
}

/** Mirrors the backend's `ItemBuilder`, which shapes what gets persisted, so
 *  the live view and the reloaded transcript agree. */
function reduceTurn(v: TurnView, e: StreamEvent): TurnView {
  switch (e.event) {
    case "chunk": {
      const f = flushToolCalls(v);
      return { ...f, response: f.response + e.data.text };
    }
    case "reasoning":
      return { ...v, reasoning: v.reasoning + e.data.text };
    case "toolStart": {
      // Text/thinking streamed before the call stays with the round it came from.
      const c = commitText(v);
      return { ...c, toolCalls: [...c.toolCalls, { name: e.data.name, arguments: e.data.arguments, isRunning: true }] };
    }
    case "toolResult": {
      // Results arrive in call order; attach to the FIRST still-running call of
      // that name so two parallel same-named calls don't both take the first result.
      let matched = false;
      return {
        ...v,
        toolCalls: v.toolCalls.map((tc) => {
          if (!matched && tc.name === e.data.name && tc.isRunning) {
            matched = true;
            return { ...tc, result: e.data.result, isRunning: false };
          }
          return tc;
        }),
      };
    }
    case "image": {
      const f = flushToolCalls(v);
      return {
        ...f,
        items: [...f.items, { id: newItemId(), type: "assistant", content: "", images: [e.data.dataUrl], ts: Date.now() }],
      };
    }
    case "usage":
      return { ...v, usage: { used: e.data.totalTokens, total: e.data.contextWindow } };
    case "done":
      return commitText(flushToolCalls(v));
    case "error": {
      const f = flushToolCalls(v);
      return {
        ...f,
        response: "",
        reasoning: "",
        items: [...f.items, { id: newItemId(), type: "error", content: e.data.message, ts: Date.now() }],
      };
    }
  }
}

/** The turn this window is following: id, last applied `seq`, and its view. */
interface TurnState {
  id: string;
  seq: number;
  view: TurnView;
}

/**
 * Shared chat session logic for both the pet window and the panel: the active
 * session's transcript, the turn running in it, and session list/new/switch/
 * delete.
 *
 * The backend owns every turn (`pet_core::turn::TurnRunner`): it persists the
 * user's message, streams the reply, injects background-task completions and
 * saves the result. This hook only starts turns and observes them, so nothing
 * that happens to a window — a tab switch, a reload, closing the panel — can
 * interrupt or lose a reply. On mount, focus and session switch it re-attaches
 * to whatever the session is doing right now.
 */
export function useChat() {
  // Each piece of session state carries a ref mirror so event-driven code (the
  // `turn` listener, focus reload) reads current values instead of stale
  // closures. Each `setX` writes the ref AND the state together — always call
  // these, never a bare React setter, so the ref can never lag the state.
  const [items, setItemsState] = useState<ChatItem[]>([]);
  const itemsRef = useRef<ChatItem[]>([]);
  const setItems = useCallback((v: ChatItem[]) => {
    itemsRef.current = v;
    setItemsState(v);
  }, []);

  const [turn, setTurnState] = useState<TurnState | null>(null);
  const turnRef = useRef<TurnState | null>(null);
  const setTurn = useCallback((v: TurnState | null) => {
    turnRef.current = v;
    setTurnState(v);
  }, []);

  // Persisted context-window occupancy (the running turn's live figure is
  // read from its view). Null until a turn has run in this session.
  const [contextUsage, setContextUsage] = useState<Usage | null>(null);

  const [sessionId, setSessionIdState] = useState("");
  const sessionIdRef = useRef("");
  const setSessionId = useCallback((v: string) => {
    sessionIdRef.current = v;
    setSessionIdState(v);
  }, []);

  const [sessionList, setSessionList] = useState<SessionMeta[]>([]);
  const [loaded, setLoaded] = useState(false);
  // Sessions with a turn in flight, for the session rail.
  const [running, setRunning] = useState<string[]>([]);

  // Catch-up bookkeeping. While an `attach_turn` is in flight, live stream
  // notices are parked in `backlog` and applied on top of the snapshot; `gen`
  // invalidates a snapshot that arrives after the session or turn moved on.
  const syncGenRef = useRef(0);
  const syncingRef = useRef(false);
  const backlogRef = useRef<StreamNotice[]>([]);
  const resetSync = () => {
    syncGenRef.current += 1;
    syncingRef.current = false;
    backlogRef.current = [];
  };

  const refreshSessionList = async () => {
    try {
      const index = await invoke<SessionIndex>("list_sessions");
      setSessionList(index.sessions);
      return index;
    } catch (e) {
      console.error("Failed to list sessions:", e);
      return null;
    }
  };

  const refreshRunning = async () => {
    try {
      setRunning(await invoke<string[]>("running_turns"));
    } catch (e) {
      console.error("Failed to list running turns:", e);
    }
  };

  // Join the turn running in `id` (if any) from wherever it is: replay the
  // backend's snapshot, then whatever streamed while we were fetching it.
  const attach = async (id: string) => {
    resetSync();
    const gen = syncGenRef.current;
    syncingRef.current = true;
    let snap: TurnSnapshot | null = null;
    try {
      snap = await invoke<TurnSnapshot | null>("attach_turn", { sessionId: id });
    } catch (e) {
      console.error("Failed to attach to turn:", e);
    }
    if (gen !== syncGenRef.current || id !== sessionIdRef.current) return; // superseded
    syncingRef.current = false;
    const backlog = backlogRef.current;
    backlogRef.current = [];
    if (!snap) {
      setTurn(null);
      return;
    }
    let view = snap.events.reduce(reduceTurn, EMPTY_VIEW);
    let seq = snap.seq;
    for (const n of backlog) {
      if (n.turnId === snap.turnId && n.seq > seq) {
        view = reduceTurn(view, n.event);
        seq = n.seq;
      }
    }
    setTurn({ id: snap.turnId, seq, view });
  };

  // Re-read the persisted transcript of the session being shown (the backend
  // appended to it: a turn opened or finished, a heartbeat message arrived).
  const reloadItems = async (id: string) => {
    try {
      const view = await invoke<SessionView>("load_session", { id });
      if (id !== sessionIdRef.current) return;
      setItems(view.items || []);
      setContextUsage(view.context_usage ?? null);
    } catch (e) {
      console.error("Failed to reload session:", e);
    }
  };

  // Show session `id`: its transcript, then the turn running in it, if any.
  const loadSessionData = async (id: string): Promise<SessionView | null> => {
    if (id !== sessionIdRef.current) {
      resetSync();
      setTurn(null);
    }
    try {
      const view = await invoke<SessionView>("load_session", { id });
      setSessionId(view.id);
      setItems(view.items || []);
      setContextUsage(view.context_usage ?? null);
      await attach(view.id);
      return view;
    } catch (e) {
      console.error("Failed to load session:", e);
      return null;
    }
  };

  const newSession = useCallback(async () => {
    try {
      const view = await invoke<SessionView>("create_session");
      resetSync();
      setTurn(null);
      setSessionId(view.id);
      setItems([]);
      setContextUsage(null);
      await refreshSessionList();
      return view.id;
    } catch (e) {
      console.error("Failed to create session:", e);
    }
  }, []);

  // Load the active (or newest) session on mount; create one if none exist.
  useEffect(() => {
    (async () => {
      try {
        refreshRunning();
        const index = await invoke<SessionIndex>("list_sessions");
        setSessionList(index.sessions);
        if (index.active_id && index.sessions.some((s) => s.id === index.active_id)) {
          await loadSessionData(index.active_id);
        } else if (index.sessions.length > 0) {
          await loadSessionData(index.sessions[index.sessions.length - 1].id);
        } else {
          await newSession();
        }
      } catch (e) {
        console.error("Failed to load sessions:", e);
        await newSession();
      }
      setLoaded(true);
    })();
  }, [newSession]);

  // Rename any session by id; the backend persists title + index meta without
  // touching messages/items.
  const renameSession = useCallback(async (id: string, title: string) => {
    const trimmed = title.trim();
    if (!id || !trimmed) return;
    try {
      await invoke("rename_session", { id, title: trimmed });
      await refreshSessionList();
    } catch (e) {
      console.error("Failed to rename session:", e);
    }
  }, []);

  // Persist the choice to the shared index BEFORE loading, so the other window's
  // focus-reload (which reads index.active_id) converges on the session we picked.
  // Switching away from a streaming session is fine: the turn runs on in the
  // backend and this window simply stops following it.
  const switchSession = useCallback(async (id: string) => {
    try {
      await invoke("set_active_session", { id });
    } catch (e) {
      console.error("Failed to set active session:", e);
    }
    await loadSessionData(id);
  }, []);

  const deleteSession = useCallback(
    async (id: string) => {
      try {
        await invoke("delete_session", { id });
        const index = await refreshSessionList();
        if (id === sessionIdRef.current) {
          if (index && index.sessions.length > 0) {
            await loadSessionData(index.sessions[index.sessions.length - 1].id);
          } else {
            await newSession();
          }
        }
      } catch (e) {
        console.error("Failed to delete session:", e);
      }
    },
    [newSession],
  );

  // Delete the selected items from the visible transcript AND from the LLM
  // context, then persist. The backend owns that mapping (`prune_session_items`)
  // because it requires understanding the stored message format, and refuses
  // while a turn is running in the session.
  const deleteItems = useCallback(async (selectedIds: string[]) => {
    if (selectedIds.length === 0) return;
    const id = sessionIdRef.current;
    if (!id) return;
    try {
      const view = await invoke<SessionView>("prune_session_items", { id, itemIds: selectedIds });
      setItems(view.items);
      await refreshSessionList();
    } catch (e) {
      console.error("Failed to delete items:", e);
    }
  }, []);

  // Start a turn. The backend persists the user item before `send_chat`
  // returns and streams the reply through the `turn` event; the bubble is shown
  // optimistically until the persisted copy is reloaded on `started`.
  const sendMessage = useCallback(async (content: string, images?: string[]) => {
    const id = sessionIdRef.current;
    if (!id || turnRef.current) return;
    const userItem: ChatItem = { id: newItemId(), type: "user", content, images, ts: Date.now() };
    setItems([...itemsRef.current, userItem]);
    try {
      const turnId = await invoke<string>("send_chat", {
        sessionId: id,
        turn: { text: content, images: images ?? [] },
      });
      // `started` usually lands first; either way the turn is now followed.
      if (!turnRef.current) setTurn({ id: turnId, seq: 0, view: EMPTY_VIEW });
    } catch (err) {
      // The turn never started (another window is mid-turn here, unreadable
      // file): nothing was persisted, so replace the bubble with the reason.
      setItems([
        ...itemsRef.current.filter((i) => i.id !== userItem.id),
        { id: newItemId(), type: "error", content: `${err}`, ts: Date.now() },
      ]);
    }
  }, []);

  // Abort the turn running in this session. The backend ends the stream with
  // `done` (partial answer kept) and finishes the turn through the normal path.
  const stopStreaming = useCallback(() => {
    const id = sessionIdRef.current;
    if (!id || !turnRef.current) return;
    invoke("cancel_chat", { sessionId: id }).catch((e) => console.error("cancel_chat failed:", e));
  }, []);

  // A turn ended and its result is on disk: swap the in-progress view for the
  // persisted transcript in one render. A newer turn may already be running
  // (a queued background completion), in which case its view is left alone.
  const finishTurn = async (n: Extract<TurnNotice, { kind: "finished" }>) => {
    resetSync();
    try {
      await reloadItems(n.sessionId);
    } finally {
      if (turnRef.current?.id === n.turnId) setTurn(null);
    }
    refreshSessionList();
  };

  // Turn activity for every session (global event; the backend never targets a
  // window). The handler is captured once, so it reads everything through refs.
  // Delivery is exactly-once per event, but a window that mounted or refocused
  // mid-turn has missed some: any gap in `seq`, or an unknown turn id, triggers a
  // re-attach, which replays the turn from the start.
  useTauriEvent<TurnNotice>("turn", (e) => {
    const n = e.payload;
    if (n.kind === "started") setRunning((r) => (r.includes(n.sessionId) ? r : [...r, n.sessionId]));
    if (n.kind === "finished") setRunning((r) => r.filter((s) => s !== n.sessionId));
    if (n.sessionId !== sessionIdRef.current) return;
    if (n.kind === "started") {
      resetSync();
      setTurn({ id: n.turnId, seq: 0, view: EMPTY_VIEW });
      reloadItems(n.sessionId); // the turn's opening item(s) are already persisted
      return;
    }
    if (n.kind === "finished") {
      finishTurn(n);
      return;
    }
    if (syncingRef.current) {
      backlogRef.current.push(n);
      return;
    }
    const t = turnRef.current;
    if (!t || t.id !== n.turnId || n.seq !== t.seq + 1) {
      attach(n.sessionId);
      backlogRef.current.push(n);
      return;
    }
    setTurn({ id: t.id, seq: n.seq, view: reduceTurn(t.view, n.event) });
  });

  // A heartbeat's `chat` tool inserts a pet message into the active session on
  // disk and emits `chat-inserted` to the active window. Reload so it shows up
  // immediately; if it's for another session, the message is already persisted
  // and surfaces when that session is opened.
  useTauriEvent<{ sessionId: string }>("chat-inserted", (e) => {
    if (e.payload.sessionId !== sessionIdRef.current) return;
    reloadItems(e.payload.sessionId);
  });

  // On focus, tell the backend this window is now active (so heartbeat
  // notifications route here) and reload the latest active conversation, so
  // the pet and panel converge on the same up-to-date history and pick up a
  // turn the other window (or the CLI) started.
  useEffect(() => {
    const win = getCurrentWindow();
    let unlisten: (() => void) | undefined;
    win
      .onFocusChanged(({ payload: focused }) => {
        if (!focused) return;
        invoke("set_active_window", { label: win.label }).catch(() => {});
        (async () => {
          try {
            refreshRunning();
            const index = await invoke<SessionIndex>("list_sessions");
            setSessionList(index.sessions);
            if (index.active_id) await loadSessionData(index.active_id);
          } catch (e) {
            console.error("Focus sync failed:", e);
          }
        })();
      })
      .then((fn) => {
        unlisten = fn;
      });
    return () => unlisten?.();
  }, []);

  // What the thread renders: the persisted transcript followed by what the
  // running turn has produced so far.
  const visibleItems = useMemo(() => (turn ? [...items, ...turn.view.items] : items), [items, turn]);

  return {
    items: visibleItems,
    isLoading: turn !== null,
    currentResponse: turn?.view.response ?? "",
    currentReasoning: turn?.view.reasoning ?? "",
    currentToolCalls: turn?.view.toolCalls ?? [],
    loaded,
    contextUsage: turn?.view.usage ?? contextUsage,
    sessionId,
    sessionList,
    runningSessions: running,
    sendMessage,
    stopStreaming,
    newSession,
    renameSession,
    switchSession,
    deleteSession,
    deleteItems,
  };
}
