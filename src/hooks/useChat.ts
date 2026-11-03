import { useState, useRef, useCallback, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import { emptyView, newItemId, type Usage } from "./streamView";
import type { ChatItem, TurnState } from "../types/chat";
import { useChatTurn } from "./useChatTurn";
import { useChatSessions } from "./useChatSessions";

export { newItemId } from "./streamView";
export { DEFAULT_SESSION_TITLE } from "../types/chat";
export type { ChatItem, ToolCall, SessionMeta } from "../types/chat";

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

  const turnFollow = useChatTurn({
    itemsRef,
    setItems,
    turnRef,
    setTurn,
    sessionIdRef,
    setSessionId,
    setContextUsage,
    refreshSessionList: () => sessions.refreshSessionList(),
  });

  const sessions = useChatSessions({
    sessionIdRef,
    setSessionId,
    setItems,
    setContextUsage,
    setTurn,
    resetSync: turnFollow.resetSync,
    loadSessionData: turnFollow.loadSessionData,
    refreshRunning: turnFollow.refreshRunning,
  });

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
      const err = await sessions.deleteItems(itemsRef.current.slice(idx).map((i) => i.id));
      if (err) {
        // Nothing was pruned (a turn started in this session meanwhile), so
        // resending would duplicate the message instead of replacing it.
        setItems([...itemsRef.current, { id: newItemId(), type: "error", content: err, ts: Date.now() }]);
        return;
      }
      await sendMessage(trimmed, images);
    },
    [sessions.deleteItems, sendMessage],
  );

  // Abort the turn running in this session. The backend ends the stream with
  // `done` (partial answer kept) and finishes the turn through the normal path.
  const stopStreaming = useCallback(() => {
    const id = sessionIdRef.current;
    if (!id || !turnRef.current) return;
    invoke("cancel_chat", { sessionId: id }).catch((e) => console.error("cancel_chat failed:", e));
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
    loaded: sessions.loaded,
    contextUsage: turn?.view.usage ?? contextUsage,
    turnStartedAt: turn?.startedAt,
    turnTokens: turn?.view.spent ?? 0,
    sessionId,
    sessionList: sessions.sessionList,
    runningSessions: turnFollow.running,
    sendMessage,
    editMessage,
    stopStreaming,
    newSession: sessions.newSession,
    renameSession: sessions.renameSession,
    switchSession: sessions.switchSession,
    deleteSession: sessions.deleteSession,
    deleteItems: sessions.deleteItems,
  };
}