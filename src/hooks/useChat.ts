import { useState, useRef, useEffect, useCallback } from "react";
import { invoke, Channel } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useI18n } from "../i18n";

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

export interface ChatItem {
  type: "user" | "assistant" | "tool" | "error" | "notification";
  content: string;
  images?: string[]; // base64 data URLs rendered in the bubble — user pastes, or tool-produced images (e.g. screenshots) on assistant items
  toolCalls?: ToolCall[];
  ts?: number; // epoch ms; present for messages created after timestamps shipped
  detail?: string; // notification items: the task's full result, shown on expand
}

interface SessionMeta {
  id: string;
  title: string;
  created_at: string;
  updated_at: string;
}

interface SessionIndex {
  active_id: string;
  sessions: SessionMeta[];
}

interface Session {
  id: string;
  title: string;
  created_at: string;
  updated_at: string;
  messages: any[];
  items: ChatItem[];
  context_usage?: { used: number; total: number } | null;
}

/** Background-task completion pushed from the backend (camelCase via serde). */
interface TaskCompletion {
  sessionId: string;
  taskId: string;
  kind: string;
  label: string;
  result: string;
}

type StreamEvent =
  | { event: "chunk"; data: { text: string } }
  | { event: "toolStart"; data: { name: string; arguments: string } }
  | { event: "toolResult"; data: { name: string; result: string } }
  | { event: "image"; data: { dataUrl: string } }
  | { event: "usage"; data: { promptTokens: number; totalTokens: number; contextWindow: number } }
  | { event: "done"; data: Record<string, never> }
  | { event: "error"; data: { message: string } };

/** True if `result` is the inline `{task_id, status:"running"}` JSON for `taskId`. */
function resultIsForTask(result: string | undefined, taskId: string): boolean {
  if (!result) return false;
  try {
    return JSON.parse(result)?.task_id === taskId;
  } catch {
    return false;
  }
}

/**
 * Replace the "后台运行中" placeholder result of the tool call that launched
 * `taskId` with the task's final result, so the rendered block flips from
 * background-running to finished. Returns the same array reference if nothing
 * matched (no re-render).
 */
function applyCompletionToItems(items: ChatItem[], taskId: string, result: string): ChatItem[] {
  let changed = false;
  const next = items.map((item) => {
    if (item.type !== "tool" || !item.toolCalls) return item;
    let itemChanged = false;
    const toolCalls = item.toolCalls.map((tc) => {
      if (resultIsForTask(tc.result, taskId)) {
        itemChanged = true;
        return { ...tc, result, isRunning: false };
      }
      return tc;
    });
    if (!itemChanged) return item;
    changed = true;
    return { ...item, toolCalls };
  });
  return changed ? next : items;
}

/**
 * Shared chat session logic for both the pet window and the panel.
 * Manages the active session (messages + rendered items with tool calls and
 * timestamps), streaming, and session list/new/switch/delete.
 *
 * The main window also listens for `background-finished` events and resumes the
 * conversation automatically with the task result (see the queue/drain below).
 */
export function useChat() {
  const { t } = useI18n();
  const [items, setItems] = useState<ChatItem[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [currentResponse, setCurrentResponse] = useState("");
  const [currentToolCalls, setCurrentToolCalls] = useState<ToolCall[]>([]);
  const [loaded, setLoaded] = useState(false);
  // Latest context-window occupancy reported by the backend (last LLM round of
  // the current turn). Null until a turn runs in this session; reset on switch.
  const [contextUsage, setContextUsage] = useState<{ used: number; total: number } | null>(null);
  // Ref mirror so saveCurrentSession (called from event-driven finish/drain)
  // persists the latest usage instead of a stale closure value.
  const contextUsageRef = useRef<{ used: number; total: number } | null>(null);
  const setUsage = (u: { used: number; total: number } | null) => {
    contextUsageRef.current = u;
    setContextUsage(u);
  };

  const [sessionId, setSessionId] = useState("");
  const [sessionTitle, setSessionTitle] = useState(DEFAULT_SESSION_TITLE);
  const [sessionList, setSessionList] = useState<SessionMeta[]>([]);
  const messagesRef = useRef<any[]>([]);

  // Refs mirroring state, so the event-driven drain reads current values
  // instead of stale closures.
  const itemsRef = useRef<ChatItem[]>([]);
  const sessionIdRef = useRef("");
  const sessionTitleRef = useRef(DEFAULT_SESSION_TITLE);
  useEffect(() => {
    itemsRef.current = items;
  }, [items]);
  useEffect(() => {
    sessionIdRef.current = sessionId;
  }, [sessionId]);
  useEffect(() => {
    sessionTitleRef.current = sessionTitle;
  }, [sessionTitle]);

  // Synchronous turn lock + pending background completions queue.
  const busyRef = useRef(false);
  const queueRef = useRef<TaskCompletion[]>([]);
  const processQueueRef = useRef<() => void>(() => {});
  // Task ids already enqueued, so a completion is never processed twice even if
  // the event is delivered more than once (e.g. a leaked duplicate listener).
  const seenTaskIdsRef = useRef<Set<string>>(new Set());

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

  // Load a session into state, returning its data for callers that need the
  // freshly-loaded items synchronously (the background drain).
  const loadSessionData = async (id: string): Promise<Session | null> => {
    try {
      const session = await invoke<Session>("load_session", { id });
      setSessionId(session.id);
      sessionIdRef.current = session.id;
      setSessionTitle(session.title);
      const loadedItems = session.items || [];
      setItems(loadedItems);
      itemsRef.current = loadedItems;
      messagesRef.current = session.messages || [];
      // Restore the persisted occupancy so the ring shows immediately, instead
      // of waiting for the next turn (or showing the session we switched from).
      setUsage(session.context_usage ?? null);
      // A completion for this session may have been deferred while it was closed.
      setTimeout(() => processQueueRef.current(), 0);
      return session;
    } catch (e) {
      console.error("Failed to load session:", e);
      return null;
    }
  };

  const loadSession = async (id: string) => {
    await loadSessionData(id);
  };

  const newSession = useCallback(async () => {
    try {
      const session = await invoke<Session>("create_session");
      setSessionId(session.id);
      setSessionTitle(session.title);
      setItems([]);
      setUsage(null);
      messagesRef.current = session.messages;
      await refreshSessionList();
      return session.id;
    } catch (e) {
      console.error("Failed to create session:", e);
    }
  }, []);

  // Load the active (or newest) session on mount; create one if none exist.
  useEffect(() => {
    (async () => {
      try {
        const index = await invoke<SessionIndex>("list_sessions");
        setSessionList(index.sessions);
        if (index.active_id && index.sessions.some((s) => s.id === index.active_id)) {
          await loadSession(index.active_id);
        } else if (index.sessions.length > 0) {
          await loadSession(index.sessions[index.sessions.length - 1].id);
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

  const saveCurrentSession = useCallback(
    async (newItems: ChatItem[]) => {
      const id = sessionIdRef.current;
      if (!id) return;
      // Read the title from the ref, not a captured value: a turn that resumes
      // after a session switch must save the active session's title, not a stale one.
      let title = sessionTitleRef.current;
      if (title === DEFAULT_SESSION_TITLE) {
        const firstUser = newItems.find((i) => i.type === "user");
        if (firstUser) {
          title = firstUser.content.slice(0, 20) + (firstUser.content.length > 20 ? "..." : "");
          setSessionTitle(title);
        }
      }
      const session: Session = {
        id,
        title,
        created_at: "", // preserved by backend
        updated_at: new Date().toISOString(),
        messages: messagesRef.current,
        items: newItems,
        context_usage: contextUsageRef.current,
      };
      try {
        await invoke("save_session", { session });
        await refreshSessionList();
      } catch (e) {
        console.error("Failed to save session:", e);
      }
    },
    [],
  );

  // Rename any session by id. If it's the active one, reflect the new title in
  // local state immediately; then persist title + index meta via the backend
  // (without touching messages/items).
  const renameSession = useCallback(async (id: string, title: string) => {
    const trimmed = title.trim();
    if (!id || !trimmed) return;
    if (id === sessionIdRef.current) {
      if (trimmed === sessionTitleRef.current) return;
      setSessionTitle(trimmed);
      sessionTitleRef.current = trimmed;
    }
    try {
      await invoke("rename_session", { id, title: trimmed });
      await refreshSessionList();
    } catch (e) {
      console.error("Failed to rename session:", e);
    }
  }, []);

  const switchSession = useCallback(async (id: string) => {
    await loadSession(id);
  }, []);

  const deleteSession = useCallback(
    async (id: string) => {
      try {
        await invoke("delete_session", { id });
        const index = await refreshSessionList();
        if (id === sessionIdRef.current) {
          if (index && index.sessions.length > 0) {
            await loadSession(index.sessions[index.sessions.length - 1].id);
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

  // The shared streaming core: messagesRef must already include the new turn's
  // input; `baseItems` is the rendered list to append streamed output onto.
  const runStream = useCallback(
    async (baseItems: ChatItem[]) => {
      busyRef.current = true;
      setIsLoading(true);
      setCurrentResponse("");
      setCurrentToolCalls([]);

      const onEvent = new Channel<StreamEvent>();
      let accumulated = "";
      let toolCalls: ToolCall[] = [];
      let finalItems = baseItems;

      // Commit the rendered list AND sync itemsRef synchronously. itemsRef is
      // otherwise only updated by a passive effect that runs AFTER the
      // setTimeout(0) drain below — so without this, a background-completion turn
      // would rebuild items from a stale ref and clobber the just-committed
      // assistant message with its own notification.
      const commit = (next: ChatItem[]) => {
        finalItems = next;
        setItems(next);
        itemsRef.current = next;
      };

      const flushToolCalls = () => {
        if (toolCalls.length > 0) {
          const snapshot = [...toolCalls];
          commit([...finalItems, { type: "tool", content: "", toolCalls: snapshot, ts: Date.now() }]);
          toolCalls = [];
          setCurrentToolCalls([]);
        }
      };

      const finish = (extra?: ChatItem) => {
        // Always re-commit so itemsRef is fresh for the drain, even when there's
        // no trailing assistant/error item (e.g. a turn that only ran tools).
        commit(extra ? [...finalItems, extra] : finalItems);
        setCurrentResponse("");
        setCurrentToolCalls([]);
        setIsLoading(false);
        busyRef.current = false;
        saveCurrentSession(finalItems);
        // Drain any background completions that arrived during this turn.
        setTimeout(() => processQueueRef.current(), 0);
      };

      onEvent.onmessage = (event: StreamEvent) => {
        if (event.event === "chunk") {
          flushToolCalls();
          accumulated += event.data.text;
          setCurrentResponse(accumulated);
        } else if (event.event === "toolStart") {
          // Preserve any assistant text streamed before the tool call.
          if (accumulated.trim()) {
            commit([...finalItems, { type: "assistant", content: accumulated, ts: Date.now() }]);
            messagesRef.current = [...messagesRef.current, { role: "assistant", content: accumulated }];
          }
          accumulated = "";
          setCurrentResponse("");
          const tc: ToolCall = { name: event.data.name, arguments: event.data.arguments, isRunning: true };
          toolCalls = [...toolCalls, tc];
          setCurrentToolCalls([...toolCalls]);
        } else if (event.event === "toolResult") {
          // Results arrive in call order; attach to the FIRST still-running call
          // of that name so two parallel same-named calls (e.g. two bash) don't
          // both receive the first result.
          let matched = false;
          toolCalls = toolCalls.map((tc) => {
            if (!matched && tc.name === event.data.name && tc.isRunning) {
              matched = true;
              return { ...tc, result: event.data.result, isRunning: false };
            }
            return tc;
          });
          setCurrentToolCalls([...toolCalls]);
        } else if (event.event === "image") {
          // A tool (e.g. screenshot) produced an image for the model to see.
          // Flush the preceding tool card, then render the image as its own
          // bubble so the owner sees what the pet saw. The data URL also lives
          // in the server's message history for the model; here it's UI-only.
          flushToolCalls();
          commit([...finalItems, { type: "assistant", content: "", images: [event.data.dataUrl], ts: Date.now() }]);
        } else if (event.event === "usage") {
          // Keep the latest round's usage; the final round carries the fullest context.
          setUsage({ used: event.data.totalTokens, total: event.data.contextWindow });
        } else if (event.event === "done") {
          flushToolCalls();
          if (accumulated.trim()) {
            messagesRef.current = [...messagesRef.current, { role: "assistant", content: accumulated }];
            finish({ type: "assistant", content: accumulated, ts: Date.now() });
          } else {
            finish();
          }
        } else if (event.event === "error") {
          finish({ type: "error", content: event.data.message, ts: Date.now() });
        }
      };

      try {
        await invoke("chat", {
          messages: messagesRef.current,
          onEvent,
          sessionId: sessionIdRef.current,
        });
      } catch (err) {
        finish({ type: "error", content: `${err}`, ts: Date.now() });
      }
    },
    [saveCurrentSession],
  );

  const sendMessage = useCallback(
    async (content: string, images?: string[]) => {
      // Respect the turn lock: a background-completion turn may be streaming even
      // when the UI isn't visibly loading. Starting a second concurrent stream
      // would interleave mutations of messagesRef and corrupt the transcript.
      if (busyRef.current) return;
      // Refresh this session from disk first, so we append onto whatever the
      // other window most recently saved instead of overwriting it (the two
      // windows share one conversation).
      if (sessionIdRef.current) await loadSessionData(sessionIdRef.current);
      // With images, send OpenAI multimodal content (text + image_url parts);
      // the litellm proxy translates this to the underlying vision model. Plain
      // text stays a bare string so existing behavior is unchanged.
      const apiContent =
        images && images.length > 0
          ? [
              ...(content ? [{ type: "text", text: content }] : []),
              ...images.map((url) => ({ type: "image_url", image_url: { url } })),
            ]
          : content;
      const userMsg = { role: "user", content: apiContent };
      messagesRef.current = [...messagesRef.current, userMsg];
      const newItems: ChatItem[] = [
        ...itemsRef.current,
        { type: "user", content, images, ts: Date.now() },
      ];
      setItems(newItems);
      await runStream(newItems);
    },
    [runStream],
  );

  // Resume the conversation with a finished background task's result. Only ever
  // called for the active session (see processQueue), so it never switches the
  // session out from under the user — completions for other sessions wait in the
  // queue until that session is opened.
  const runNotificationTurn = useCallback(
    async (c: TaskCompletion) => {
      const label = c.label || c.kind;
      // Flip the originating tool call from "后台运行中" to its final result.
      const base = applyCompletionToItems(itemsRef.current, c.taskId, c.result);
      messagesRef.current = [
        ...messagesRef.current,
        { role: "user", content: t("chat.bgTaskDoneContent", { label, result: c.result }) },
      ];
      const newItems: ChatItem[] = [
        ...base,
        { type: "notification", content: t("chat.bgTaskDone", { label }), detail: c.result, ts: Date.now() },
      ];
      setItems(newItems);
      itemsRef.current = newItems;
      await runStream(newItems);
    },
    [runStream, t],
  );

  // Drain the queue one turn at a time when idle. Only completions for the
  // currently-open session (or session-less ones) are processed; others stay
  // queued so the user is never yanked to a different conversation.
  const processQueue = useCallback(() => {
    if (busyRef.current) return;
    const idx = queueRef.current.findIndex(
      (c) => !c.sessionId || c.sessionId === sessionIdRef.current,
    );
    if (idx === -1) return;
    const next = queueRef.current[idx];
    queueRef.current = [
      ...queueRef.current.slice(0, idx),
      ...queueRef.current.slice(idx + 1),
    ];
    busyRef.current = true; // claim the lock before the async turn starts
    runNotificationTurn(next).catch((e) => {
      console.error("Notification turn failed:", e);
      busyRef.current = false;
      setTimeout(() => processQueueRef.current(), 0);
    });
  }, [runNotificationTurn]);

  useEffect(() => {
    processQueueRef.current = processQueue;
  }, [processQueue]);

  // Both windows listen, but the backend emits `background-finished` only to the
  // ACTIVE window (see `active_window_label` in window.rs), so a completion is
  // injected into the conversation exactly once — in the window the user is
  // looking at.
  //
  // The listener is ALWAYS kept registered (no self-cancelling on teardown). The
  // duplicate-handling problem is solved by the `seenTaskIdsRef` dedup below, not
  // by trying to guarantee a single listener — a cancel-after-await dance is
  // fragile under StrictMode / Vite HMR and can leave zero listeners. Worst case
  // a remount leaks one extra listener; dedup makes that harmless.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    listen<TaskCompletion>("background-finished", (e) => {
      const c = e.payload;
      if (seenTaskIdsRef.current.has(c.taskId)) return; // already handled once
      seenTaskIdsRef.current.add(c.taskId);
      queueRef.current = [...queueRef.current, c];
      processQueueRef.current();
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, []);

  // A heartbeat's `chat` tool inserts a pet message into the active session on
  // disk and emits `chat-inserted` to the active window. Reload so it shows up
  // immediately; if we're mid-turn or it's for another session, the message is
  // already persisted and surfaces on the next focus/reload.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    listen<{ sessionId: string }>("chat-inserted", (e) => {
      if (busyRef.current) return;
      if (e.payload.sessionId !== sessionIdRef.current) return;
      loadSessionData(sessionIdRef.current);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, []);

  // On focus, tell the backend this window is now active (so completion
  // notifications route here) and reload the latest active conversation, so the
  // pet and panel converge on the same up-to-date history (req: refresh on focus,
  // and see messages typed in the other window). Skipped while a turn is
  // streaming so we never clobber in-flight state.
  useEffect(() => {
    const win = getCurrentWindow();
    let unlisten: (() => void) | undefined;
    win
      .onFocusChanged(({ payload: focused }) => {
        if (!focused) return;
        invoke("set_active_window", { label: win.label }).catch(() => {});
        if (busyRef.current) return;
        (async () => {
          try {
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

  return {
    items,
    isLoading,
    currentResponse,
    currentToolCalls,
    loaded,
    contextUsage,
    sessionId,
    sessionTitle,
    sessionList,
    sendMessage,
    newSession,
    renameSession,
    switchSession,
    deleteSession,
  };
}
