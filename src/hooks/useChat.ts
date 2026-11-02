import { useState, useRef, useEffect, useCallback, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useTauriEvent } from "./useTauriEvent";
import { applyStream, emptyView, newItemId, type StreamEvent, type StreamView, type Usage } from "./streamView";

export { newItemId } from "./streamView";

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

interface SessionIndex {
  active_id: string;
  sessions: SessionMeta[];
}

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

/** Turn activity from the backend (`turn` event, camelCase via serde). One
 *  stream for every session; this hook keeps what concerns its own. */
type TurnOrigin =
  | { type: "user"; text: string }
  | { type: "completion"; label: string };

type TurnNotice =
  | { kind: "started"; sessionId: string; turnId: string; origin: TurnOrigin; startedAt: number }
  | { kind: "stream"; sessionId: string; turnId: string; seq: number; event: StreamEvent }
  | { kind: "finished"; sessionId: string; turnId: string };

/** Everything a running turn has produced (`attach_turn`): the backend's own
 *  committed items, the still-streaming tail, and the turn's opening origin. */
interface TurnSnapshot {
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

/** The opening line a turn started elsewhere renders: the user's message, or
 *  the background-task notification line. */
function openingItem(origin: TurnOrigin): ChatItem {
  if (origin.type === "user") {
    return { id: newItemId(), type: "user", content: origin.text, ts: Date.now() };
  }
  return {
    id: newItemId(),
    type: "notification",
    content: `后台任务完成：${origin.label}`,
    label: origin.label,
    ts: Date.now(),
  };
}

/** True when the persisted transcript already carries this turn's opening line
 *  (a reload usually lands before or during `attach`). */
function hasOpeningItem(items: ChatItem[], origin: TurnOrigin): boolean {
  return items.some((i) =>
    origin.type === "user"
      ? i.type === "user" && i.content === origin.text
      : i.type === "notification" && i.label === origin.label,
  );
}

/** The turn this window is following: id, last applied `seq`, and its view. */
interface TurnState {
  id: string;
  /** Epoch ms the backend stamped on the turn — never `Date.now()` here, so a
   *  window that re-attaches mid-turn shows the real elapsed time, not zero. */
  startedAt: number;
  seq: number;
  view: StreamView;
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

  // Catch-up bookkeeping. While an `attach_turn` is in flight, this window
  // can't apply stream events (it has no view yet), so it remembers the highest
  // `seq` it saw and re-attaches if the snapshot turned out to be older. `gen`
  // invalidates a snapshot that arrives after the session or turn moved on.
  const syncGenRef = useRef(0);
  const syncingRef = useRef(false);
  const pendingSeqRef = useRef<{ turnId: string; seq: number } | null>(null);
  // Id of an opening bubble synthesized because the persisted copy hadn't
  // loaded yet; dropped as soon as the real item arrives.
  const originEchoRef = useRef<string | null>(null);
  // Reading through a function keeps TypeScript from narrowing the ref to the
  // value it had at the last synchronous assignment (it changes during `await`).
  const pendingNow = () => pendingSeqRef.current;

  // Echo a turn's opening line into `view` unless the persisted transcript
  // already has it, remembering the synthetic id so the later reload can drop it.
  const echoOrigin = (origin: TurnOrigin, view: StreamView) => {
    if (hasOpeningItem(itemsRef.current, origin)) return;
    const item = openingItem(origin);
    originEchoRef.current = item.id;
    view.items = [item, ...view.items];
  };
  const resetSync = () => {
    syncGenRef.current += 1;
    syncingRef.current = false;
    pendingSeqRef.current = null;
    originEchoRef.current = null;
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

  // Join the turn running in `id` (if any) from wherever it is. The snapshot is
  // the backend's own view, so no event replay is needed; if events arrived
  // while the call was in flight, retry once or twice to catch up.
  const attach = async (id: string) => {
    resetSync();
    const gen = syncGenRef.current;
    syncingRef.current = true;
    let snap: TurnSnapshot | null = null;
    for (let attempt = 0; attempt < 3; attempt++) {
      pendingSeqRef.current = null;
      try {
        snap = await invoke<TurnSnapshot | null>("attach_turn", { sessionId: id });
      } catch (e) {
        console.error("Failed to attach to turn:", e);
        snap = null;
      }
      if (gen !== syncGenRef.current || id !== sessionIdRef.current) return; // superseded
      if (!snap) break;
      const pending = pendingNow();
      if (!pending || pending.turnId !== snap.turnId || pending.seq <= snap.seq) break;
    }
    syncingRef.current = false;
    pendingSeqRef.current = null;
    if (!snap) {
      setTurn(null);
      return;
    }
    const view: StreamView = {
      items: snap.items,
      streaming: snap.response,
      reasoning: snap.reasoning,
      toolCalls: snap.toolCalls,
      usage: snap.usage ?? null,
      spent: snap.spent,
      running: true,
    };
    // A turn started in another window has no optimistic bubble here; echo its
    // opening line unless the persisted transcript already carries it.
    echoOrigin(snap.origin, view);
    setTurn({ id: snap.turnId, startedAt: snap.startedAt, seq: snap.seq, view });
  };

  // Re-read the persisted transcript of the session being shown (the backend
  // appended to it: a turn opened or finished, a heartbeat message arrived).
  const reloadItems = async (id: string) => {
    try {
      const view = await invoke<SessionView>("load_session", { id });
      if (id !== sessionIdRef.current) return;
      setItems(view.items || []);
      setContextUsage(view.context_usage ?? null);
      const echo = originEchoRef.current;
      if (echo) {
        originEchoRef.current = null;
        const t = turnRef.current;
        if (t) setTurn({ ...t, view: { ...t.view, items: t.view.items.filter((i) => i.id !== echo) } });
      }
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
      const echo = originEchoRef.current;
      if (echo) {
        originEchoRef.current = null;
        const t = turnRef.current;
        if (t) setTurn({ ...t, view: { ...t.view, items: t.view.items.filter((i) => i.id !== echo) } });
      }
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
  // Returns the backend's refusal (a turn is running in this session,
  // unreadable file) so a caller that acts on the pruned transcript — the
  // edit-and-resend below — can stop instead of building on a stale view.
  const deleteItems = useCallback(async (selectedIds: string[]): Promise<string | null> => {
    if (selectedIds.length === 0) return null;
    const id = sessionIdRef.current;
    if (!id) return null;
    try {
      const view = await invoke<SessionView>("prune_session_items", { id, itemIds: selectedIds });
      setItems(view.items);
      await refreshSessionList();
      return null;
    } catch (e) {
      console.error("Failed to delete items:", e);
      return `${e}`;
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
      if (!turnRef.current) setTurn({ id: turnId, startedAt: Date.now(), seq: 0, view: emptyView() });
    } catch (err) {
      // The turn never started (another window is mid-turn here, unreadable
      // file): nothing was persisted, so replace the bubble with the reason.
      setItems([
        ...itemsRef.current.filter((i) => i.id !== userItem.id),
        { id: newItemId(), type: "error", content: `${err}`, ts: Date.now() },
      ]);
    }
  }, []);

  // Rewrite a message already sent and send it again: everything from it
  // onward leaves the transcript AND the LLM context first, so the reply is
  // produced from the edited history rather than from the original next to it.
  // Its images ride along — the edit is of the text, not of the attachment.
  const editMessage = useCallback(
    async (itemId: string, text: string) => {
      const trimmed = text.trim();
      if (!trimmed || !sessionIdRef.current) return;
      const idx = itemsRef.current.findIndex((i) => i.id === itemId);
      if (idx < 0) return;
      const { images } = itemsRef.current[idx];
      const err = await deleteItems(itemsRef.current.slice(idx).map((i) => i.id));
      if (err) {
        // Nothing was pruned (a turn started in this session meanwhile), so
        // resending would duplicate the message instead of replacing it.
        setItems([...itemsRef.current, { id: newItemId(), type: "error", content: err, ts: Date.now() }]);
        return;
      }
      await sendMessage(trimmed, images);
    },
    [deleteItems, sendMessage],
  );

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
      const view = emptyView();
      // This window may not have the opening bubble yet (the persistence reload
      // is async); show it immediately, deduped by content/label.
      echoOrigin(n.origin, view);
      setTurn({ id: n.turnId, startedAt: n.startedAt, seq: 0, view });
      reloadItems(n.sessionId); // the turn's opening item(s) are already persisted
      return;
    }
    if (n.kind === "finished") {
      finishTurn(n);
      return;
    }
    if (syncingRef.current) {
      const pending = pendingSeqRef.current;
      if (!pending || pending.turnId !== n.turnId || n.seq > pending.seq) {
        pendingSeqRef.current = { turnId: n.turnId, seq: n.seq };
      }
      return;
    }
    const t = turnRef.current;
    if (!t || t.id !== n.turnId || n.seq !== t.seq + 1) {
      attach(n.sessionId);
      return;
    }
    setTurn({ ...t, seq: n.seq, view: applyStream(t.view, n.event) });
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
    currentResponse: turn?.view.streaming ?? "",
    currentReasoning: turn?.view.reasoning ?? "",
    currentToolCalls: turn?.view.toolCalls ?? [],
    loaded,
    contextUsage: turn?.view.usage ?? contextUsage,
    turnStartedAt: turn?.startedAt,
    turnTokens: turn?.view.spent ?? 0,
    sessionId,
    sessionList,
    runningSessions: running,
    sendMessage,
    editMessage,
    stopStreaming,
    newSession,
    renameSession,
    switchSession,
    deleteSession,
    deleteItems,
  };
}
