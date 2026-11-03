import { useCallback, useEffect, useState, type MutableRefObject } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEvent } from "../lib/events";
import type { Usage } from "./streamView";
import type { ChatItem, SessionIndex, SessionMeta, SessionView, TurnState } from "../types/chat";

interface Args {
  sessionIdRef: MutableRefObject<string>;
  setSessionId: (v: string) => void;
  setItems: (v: ChatItem[]) => void;
  setContextUsage: (v: Usage | null) => void;
  setTurn: (v: TurnState | null) => void;
  resetSync: () => void;
  loadSessionData: (id: string) => Promise<SessionView | null>;
  refreshRunning: () => Promise<void>;
}

/**
 * The session list and every operation on it: refresh, create, rename, switch,
 * delete, and pruning items from the visible transcript. Loading a session's
 * transcript + running turn is delegated to the turn-follow hook.
 */
export function useChatSessions({
  sessionIdRef,
  setSessionId,
  setItems,
  setContextUsage,
  setTurn,
  resetSync,
  loadSessionData,
  refreshRunning,
}: Args) {
  const [sessionList, setSessionList] = useState<SessionMeta[]>([]);
  const [loaded, setLoaded] = useState(false);

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

  // On focus, tell the backend this window is now active (so heartbeat
  // notifications route here) and reload the latest active conversation, so
  // the pet and panel converge on the same up-to-date history and pick up a
  // turn the other window (or the CLI) started.
  useEvent("tauri://focus", () => {
    invoke("set_active_window", { label: getCurrentWindow().label }).catch(() => {});
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
  });

  return {
    sessionList,
    loaded,
    refreshSessionList,
    newSession,
    renameSession,
    switchSession,
    deleteSession,
    deleteItems,
  };
}