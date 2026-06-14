import { useState, useRef, useEffect, useCallback } from "react";
import { invoke, Channel } from "@tauri-apps/api/core";

export interface ToolCall {
  name: string;
  arguments: string;
  result?: string;
  isRunning: boolean;
}

export interface ChatItem {
  type: "user" | "assistant" | "tool" | "error";
  content: string;
  toolCalls?: ToolCall[];
  ts?: number; // epoch ms; present for messages created after timestamps shipped
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
}

type StreamEvent =
  | { event: "chunk"; data: { text: string } }
  | { event: "toolStart"; data: { name: string; arguments: string } }
  | { event: "toolResult"; data: { name: string; result: string } }
  | { event: "done"; data: Record<string, never> }
  | { event: "error"; data: { message: string } };

/**
 * Shared chat session logic for both the pet window and the panel.
 * Manages the active session (messages + rendered items with tool calls and
 * timestamps), streaming, and session list/new/switch/delete.
 */
export function useChat() {
  const [items, setItems] = useState<ChatItem[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [currentResponse, setCurrentResponse] = useState("");
  const [currentToolCalls, setCurrentToolCalls] = useState<ToolCall[]>([]);
  const [loaded, setLoaded] = useState(false);

  const [sessionId, setSessionId] = useState("");
  const [sessionTitle, setSessionTitle] = useState("新会话");
  const [sessionList, setSessionList] = useState<SessionMeta[]>([]);
  const messagesRef = useRef<any[]>([]);

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

  const loadSession = async (id: string) => {
    try {
      const session = await invoke<Session>("load_session", { id });
      setSessionId(session.id);
      setSessionTitle(session.title);
      setItems(session.items || []);
      messagesRef.current = session.messages || [];
    } catch (e) {
      console.error("Failed to load session:", e);
    }
  };

  const newSession = useCallback(async () => {
    try {
      const session = await invoke<Session>("create_session");
      setSessionId(session.id);
      setSessionTitle(session.title);
      setItems([]);
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
      if (!sessionId) return;
      let title = sessionTitle;
      if (title === "新会话") {
        const firstUser = newItems.find((i) => i.type === "user");
        if (firstUser) {
          title = firstUser.content.slice(0, 20) + (firstUser.content.length > 20 ? "..." : "");
          setSessionTitle(title);
        }
      }
      const session: Session = {
        id: sessionId,
        title,
        created_at: "", // preserved by backend
        updated_at: new Date().toISOString(),
        messages: messagesRef.current,
        items: newItems,
      };
      try {
        await invoke("save_session", { session });
        await refreshSessionList();
      } catch (e) {
        console.error("Failed to save session:", e);
      }
    },
    [sessionId, sessionTitle],
  );

  const switchSession = useCallback(async (id: string) => {
    await loadSession(id);
  }, []);

  const deleteSession = useCallback(
    async (id: string) => {
      try {
        await invoke("delete_session", { id });
        const index = await refreshSessionList();
        if (id === sessionId) {
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
    [sessionId, newSession],
  );

  const sendMessage = useCallback(
    async (content: string) => {
      const userTs = Date.now();
      const userMsg = { role: "user", content };
      messagesRef.current = [...messagesRef.current, userMsg];
      const newItems: ChatItem[] = [...items, { type: "user", content, ts: userTs }];
      setItems(newItems);
      setIsLoading(true);
      setCurrentResponse("");
      setCurrentToolCalls([]);

      const onEvent = new Channel<StreamEvent>();
      let accumulated = "";
      let toolCalls: ToolCall[] = [];
      let finalItems = newItems;

      const flushToolCalls = () => {
        if (toolCalls.length > 0) {
          const snapshot = [...toolCalls];
          finalItems = [...finalItems, { type: "tool", content: "", toolCalls: snapshot, ts: Date.now() }];
          setItems(finalItems);
          toolCalls = [];
          setCurrentToolCalls([]);
        }
      };

      onEvent.onmessage = (event: StreamEvent) => {
        if (event.event === "chunk") {
          flushToolCalls();
          accumulated += event.data.text;
          setCurrentResponse(accumulated);
        } else if (event.event === "toolStart") {
          // Preserve any assistant text streamed before the tool call.
          if (accumulated.trim()) {
            finalItems = [...finalItems, { type: "assistant", content: accumulated, ts: Date.now() }];
            setItems(finalItems);
            messagesRef.current = [...messagesRef.current, { role: "assistant", content: accumulated }];
          }
          accumulated = "";
          setCurrentResponse("");
          const tc: ToolCall = { name: event.data.name, arguments: event.data.arguments, isRunning: true };
          toolCalls = [...toolCalls, tc];
          setCurrentToolCalls([...toolCalls]);
        } else if (event.event === "toolResult") {
          toolCalls = toolCalls.map((tc) =>
            tc.name === event.data.name && tc.isRunning
              ? { ...tc, result: event.data.result, isRunning: false }
              : tc,
          );
          setCurrentToolCalls([...toolCalls]);
        } else if (event.event === "done") {
          flushToolCalls();
          if (accumulated.trim()) {
            finalItems = [...finalItems, { type: "assistant", content: accumulated, ts: Date.now() }];
            setItems(finalItems);
            messagesRef.current = [...messagesRef.current, { role: "assistant", content: accumulated }];
          }
          setCurrentResponse("");
          setIsLoading(false);
          saveCurrentSession(finalItems);
        } else if (event.event === "error") {
          finalItems = [...finalItems, { type: "error", content: event.data.message, ts: Date.now() }];
          setItems(finalItems);
          setCurrentResponse("");
          setCurrentToolCalls([]);
          setIsLoading(false);
          saveCurrentSession(finalItems);
        }
      };

      try {
        await invoke("chat", { messages: messagesRef.current, onEvent });
      } catch (err) {
        finalItems = [...finalItems, { type: "error", content: `${err}`, ts: Date.now() }];
        setItems(finalItems);
        setIsLoading(false);
        saveCurrentSession(finalItems);
      }
    },
    [items, saveCurrentSession],
  );

  return {
    items,
    isLoading,
    currentResponse,
    currentToolCalls,
    loaded,
    sessionId,
    sessionTitle,
    sessionList,
    sendMessage,
    newSession,
    switchSession,
    deleteSession,
  };
}
