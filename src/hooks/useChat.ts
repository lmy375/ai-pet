import { useState, useCallback, useEffect, useRef } from "react";
import { invoke, Channel } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface ChatMessage {
  role: "user" | "assistant" | "system" | "tool";
  content: string;
}

interface ChatItem {
  type: "user" | "assistant" | "tool" | "error";
  content: string;
}

interface SessionIndex {
  active_id: string;
  sessions: { id: string }[];
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

export function useChat(systemPrompt: string) {
  const [messages, setMessages] = useState<ChatMessage[]>([
    { role: "system", content: systemPrompt },
  ]);
  const [currentResponse, setCurrentResponse] = useState("");
  const [toolStatus, setToolStatus] = useState("");
  const [isLoading, setIsLoading] = useState(false);
  const sessionIdRef = useRef<string>("");
  const itemsRef = useRef<ChatItem[]>([]);
  const prevPrompt = useRef(systemPrompt);

  // Load active session on mount
  useEffect(() => {
    (async () => {
      try {
        const index = await invoke<SessionIndex>("list_sessions");
        if (index.sessions.length > 0) {
          // Always use the newest (last created) session
          const latest = index.sessions[index.sessions.length - 1];
          const session = await invoke<Session>("load_session", { id: latest.id });
          sessionIdRef.current = session.id;
          itemsRef.current = session.items || [];
          if (session.messages && session.messages.length > 0) {
            setMessages(session.messages as ChatMessage[]);
          }
        } else {
          const session = await invoke<Session>("create_session");
          sessionIdRef.current = session.id;
          itemsRef.current = [];
        }
      } catch (e) {
        console.error("Failed to load session for pet chat:", e);
      }
    })();
  }, []);

  // Reset conversation when system prompt changes
  useEffect(() => {
    if (prevPrompt.current !== systemPrompt) {
      prevPrompt.current = systemPrompt;
      setMessages([{ role: "system", content: systemPrompt }]);
      setCurrentResponse("");
      setToolStatus("");
      setIsLoading(false);
    }
  }, [systemPrompt]);

  // Listen for proactive (pet-initiated) messages — backend already persisted them.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    (async () => {
      unlisten = await listen<{ text: string; timestamp: string }>(
        "proactive-message",
        (event) => {
          const text = event.payload.text;
          if (!text) return;
          const assistantMsg: ChatMessage = { role: "assistant", content: text };
          setMessages((prev) => [...prev, assistantMsg]);
          itemsRef.current = [...itemsRef.current, { type: "assistant", content: text }];
        },
      );
    })();
    return () => {
      if (unlisten) unlisten();
    };
  }, []);

  const saveSession = useCallback(
    async (msgs: ChatMessage[], items: ChatItem[]) => {
      if (!sessionIdRef.current) return;
      const firstUser = items.find((i) => i.type === "user");
      const title = firstUser
        ? firstUser.content.slice(0, 20) + (firstUser.content.length > 20 ? "..." : "")
        : "新会话";
      try {
        await invoke("save_session", {
          session: {
            id: sessionIdRef.current,
            title,
            created_at: "",
            updated_at: new Date().toISOString(),
            messages: msgs,
            items,
          },
        });
      } catch (e) {
        console.error("Failed to save session:", e);
      }
    },
    [],
  );

  const sendMessage = useCallback(
    async (content: string) => {
      const userMsg: ChatMessage = { role: "user", content };
      const updatedMessages = [...messages, userMsg];
      setMessages(updatedMessages);
      setIsLoading(true);
      setCurrentResponse("");
      setToolStatus("");

      // Track items for session saving
      itemsRef.current = [...itemsRef.current, { type: "user", content }];

      const onEvent = new Channel<StreamEvent>();
      let accumulated = "";

      onEvent.onmessage = (event: StreamEvent) => {
        if (event.event === "chunk") {
          accumulated += event.data.text;
          setCurrentResponse(accumulated);
          setToolStatus("");
        } else if (event.event === "toolStart") {
          accumulated = "";
          setCurrentResponse("");
        } else if (event.event === "toolResult") {
          setToolStatus(`✅ ${event.data.name} done`);
        } else if (event.event === "done") {
          if (accumulated.trim()) {
            const assistantMsg: ChatMessage = { role: "assistant", content: accumulated };
            const newMsgs = [...updatedMessages, assistantMsg];
            setMessages(newMsgs);
            itemsRef.current = [...itemsRef.current, { type: "assistant", content: accumulated }];
            saveSession(newMsgs, itemsRef.current);
          }
          setCurrentResponse("");
          setToolStatus("");
          setIsLoading(false);
        } else if (event.event === "error") {
          setCurrentResponse(`出错了: ${event.data.message}`);
          itemsRef.current = [...itemsRef.current, { type: "error", content: event.data.message }];
          saveSession(updatedMessages, itemsRef.current);
          setToolStatus("");
          setIsLoading(false);
        }
      };

      try {
        await invoke("chat", {
          messages: updatedMessages,
          onEvent,
        });
      } catch (err) {
        setCurrentResponse(`出错了: ${err}`);
        setToolStatus("");
        setIsLoading(false);
      }
    },
    [messages, saveSession],
  );

  const lastAssistantMsg = [...messages]
    .reverse()
    .find((m) => m.role === "assistant");
  const displayMessage = currentResponse || lastAssistantMsg?.content || "";
  const showBubble = isLoading || !!lastAssistantMsg;

  return {
    messages,
    currentResponse,
    toolStatus,
    isLoading,
    sendMessage,
    displayMessage,
    showBubble,
  };
}
