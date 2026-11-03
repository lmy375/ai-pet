import { useRef, useState, type MutableRefObject } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTauriEvent } from "./useTauriEvent";
import { applyStream, emptyView, newItemId, type StreamView, type Usage } from "./streamView";
import type { ChatItem, SessionIndex, SessionView, TurnNotice, TurnOrigin, TurnSnapshot, TurnState } from "../types/chat";

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

interface Args {
  itemsRef: MutableRefObject<ChatItem[]>;
  setItems: (v: ChatItem[]) => void;
  turnRef: MutableRefObject<TurnState | null>;
  setTurn: (v: TurnState | null) => void;
  sessionIdRef: MutableRefObject<string>;
  setSessionId: (v: string) => void;
  setContextUsage: (v: Usage | null) => void;
  /** Refresh the session rail after a turn ends (owned by the sessions hook). */
  refreshSessionList: () => Promise<SessionIndex | null>;
}

/**
 * Follows the turn running in the current session: `attach` catches up on a turn
 * started elsewhere, the `turn` listener folds live events, and a finished turn
 * swaps the live view for the persisted transcript.
 *
 * This is the subtlest part of chat: `attach` and live events race, so the hook
 * remembers the highest `seq` seen while a snapshot is in flight and re-attaches
 * if that snapshot turned out to be older. Do not reorder or simplify it.
 */
export function useChatTurn({
  itemsRef,
  setItems,
  turnRef,
  setTurn,
  sessionIdRef,
  setSessionId,
  setContextUsage,
  refreshSessionList,
}: Args) {
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

  return { running, refreshRunning, resetSync, attach, reloadItems, loadSessionData };
}