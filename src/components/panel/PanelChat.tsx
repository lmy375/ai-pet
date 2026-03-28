import { useState, useRef, useEffect, useCallback } from "react";
import { invoke, Channel } from "@tauri-apps/api/core";
import { ToolCallBlock } from "./ToolCallBlock";

interface ToolCall {
  name: string;
  arguments: string;
  result?: string;
  isRunning: boolean;
}

interface ChatItem {
  type: "user" | "assistant" | "tool" | "error";
  content: string;
  toolCalls?: ToolCall[];
}

type StreamEvent =
  | { event: "chunk"; data: { text: string } }
  | { event: "toolStart"; data: { name: string; arguments: string } }
  | { event: "toolResult"; data: { name: string; result: string } }
  | { event: "done"; data: Record<string, never> }
  | { event: "error"; data: { message: string } };

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

export function PanelChat() {
  const [items, setItems] = useState<ChatItem[]>([]);
  const [input, setInput] = useState("");
  const [isLoading, setIsLoading] = useState(false);
  const [currentResponse, setCurrentResponse] = useState("");
  const [currentToolCalls, setCurrentToolCalls] = useState<ToolCall[]>([]);
  const scrollRef = useRef<HTMLDivElement>(null);
  const messagesRef = useRef<any[]>([]);

  // Session state
  const [sessionId, setSessionId] = useState<string>("");
  const [sessionTitle, setSessionTitle] = useState("新会话");
  const [sessionList, setSessionList] = useState<SessionMeta[]>([]);
  const [showSessionList, setShowSessionList] = useState(false);
  const [loaded, setLoaded] = useState(false);

  // Load sessions on mount
  useEffect(() => {
    (async () => {
      try {
        const index = await invoke<SessionIndex>("list_sessions");
        setSessionList(index.sessions);

        if (index.active_id && index.sessions.some((s) => s.id === index.active_id)) {
          await loadSession(index.active_id);
        } else if (index.sessions.length > 0) {
          // Load the most recent session
          const last = index.sessions[index.sessions.length - 1];
          await loadSession(last.id);
        } else {
          // No sessions, create one
          await handleNewSession();
        }
      } catch (e) {
        console.error("Failed to load sessions:", e);
        await handleNewSession();
      }
      setLoaded(true);
    })();
  }, []);

  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [items, currentResponse, currentToolCalls]);

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

  const saveCurrentSession = useCallback(
    async (newItems: ChatItem[]) => {
      if (!sessionId) return;

      // Auto-generate title from first user message
      let title = sessionTitle;
      if (title === "新会话") {
        const firstUser = newItems.find((i) => i.type === "user");
        if (firstUser) {
          title = firstUser.content.slice(0, 20) + (firstUser.content.length > 20 ? "..." : "");
          setSessionTitle(title);
        }
      }

      const now = new Date().toISOString();
      const session: Session = {
        id: sessionId,
        title,
        created_at: "", // preserved by backend
        updated_at: now,
        messages: messagesRef.current,
        items: newItems,
      };

      try {
        await invoke("save_session", { session });
        // Refresh session list to reflect updated title
        const index = await invoke<SessionIndex>("list_sessions");
        setSessionList(index.sessions);
      } catch (e) {
        console.error("Failed to save session:", e);
      }
    },
    [sessionId, sessionTitle],
  );

  const handleNewSession = async () => {
    try {
      const session = await invoke<Session>("create_session");
      setSessionId(session.id);
      setSessionTitle(session.title);
      setItems([]);
      messagesRef.current = session.messages;
      setShowSessionList(false);

      // Refresh session list
      const index = await invoke<SessionIndex>("list_sessions");
      setSessionList(index.sessions);
    } catch (e) {
      console.error("Failed to create session:", e);
    }
  };

  const switchSession = async (id: string) => {
    await loadSession(id);
    setShowSessionList(false);
  };

  const handleDeleteSession = async (id: string) => {
    try {
      await invoke("delete_session", { id });
      const index = await invoke<SessionIndex>("list_sessions");
      setSessionList(index.sessions);

      // If deleted the current session, switch to another or create new
      if (id === sessionId) {
        if (index.sessions.length > 0) {
          const last = index.sessions[index.sessions.length - 1];
          await loadSession(last.id);
        } else {
          await handleNewSession();
        }
      }
    } catch (e) {
      console.error("Failed to delete session:", e);
    }
  };

  const sendMessage = useCallback(
    async (content: string) => {
      const userMsg = { role: "user", content };
      messagesRef.current = [...messagesRef.current, userMsg];
      const newItems = [...items, { type: "user" as const, content }];
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
          finalItems = [...finalItems, { type: "tool" as const, content: "", toolCalls: snapshot }];
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
          if (accumulated.trim()) {
            finalItems = [...finalItems, { type: "assistant", content: accumulated }];
            setItems(finalItems);
          }
          accumulated = "";
          setCurrentResponse("");
          const tc: ToolCall = {
            name: event.data.name,
            arguments: event.data.arguments,
            isRunning: true,
          };
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
            finalItems = [...finalItems, { type: "assistant", content: accumulated }];
            setItems(finalItems);
            messagesRef.current = [
              ...messagesRef.current,
              { role: "assistant", content: accumulated },
            ];
          }
          setCurrentResponse("");
          setIsLoading(false);
          // Auto-save after completion
          saveCurrentSession(finalItems);
        } else if (event.event === "error") {
          finalItems = [...finalItems, { type: "error", content: event.data.message }];
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
        finalItems = [...finalItems, { type: "error" as const, content: `${err}` }];
        setItems(finalItems);
        setIsLoading(false);
        saveCurrentSession(finalItems);
      }
    },
    [items, saveCurrentSession],
  );

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!input.trim() || isLoading) return;
    sendMessage(input.trim());
    setInput("");
  };

  if (!loaded) {
    return (
      <div style={{ display: "flex", alignItems: "center", justifyContent: "center", height: "100%", color: "#94a3b8" }}>
        加载中...
      </div>
    );
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%" }}>
      {/* Session header bar */}
      <div style={sessionBarStyle}>
        <div
          style={{ display: "flex", alignItems: "center", gap: "8px", flex: 1, cursor: "pointer", minWidth: 0 }}
          onClick={() => setShowSessionList(!showSessionList)}
        >
          <span style={{ fontSize: "13px", fontWeight: 600, color: "#1e293b", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
            {sessionTitle}
          </span>
          <span style={{ fontSize: "10px", color: "#94a3b8" }}>{showSessionList ? "▲" : "▼"}</span>
        </div>
        <button onClick={handleNewSession} style={newSessionBtnStyle} title="新建会话">
          + 新会话
        </button>
      </div>

      {/* Session list dropdown */}
      {showSessionList && (
        <div style={sessionDropdownStyle}>
          {sessionList.length === 0 ? (
            <div style={{ padding: "12px", textAlign: "center", color: "#94a3b8", fontSize: "12px" }}>
              暂无历史会话
            </div>
          ) : (
            [...sessionList].reverse().map((s) => (
              <div
                key={s.id}
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: "8px",
                  padding: "8px 12px",
                  cursor: "pointer",
                  background: s.id === sessionId ? "#f0f9ff" : "transparent",
                  borderBottom: "1px solid #f1f5f9",
                }}
              >
                <div
                  style={{ flex: 1, minWidth: 0 }}
                  onClick={() => switchSession(s.id)}
                >
                  <div style={{ fontSize: "13px", color: "#1e293b", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", fontWeight: s.id === sessionId ? 600 : 400 }}>
                    {s.title}
                  </div>
                  <div style={{ fontSize: "11px", color: "#94a3b8" }}>
                    {s.updated_at.split("T")[0]}
                  </div>
                </div>
                <button
                  onClick={(e) => {
                    e.stopPropagation();
                    handleDeleteSession(s.id);
                  }}
                  style={{ padding: "2px 6px", borderRadius: "4px", border: "none", background: "#fee2e2", color: "#dc2626", fontSize: "11px", cursor: "pointer" }}
                  title="删除会话"
                >
                  删除
                </button>
              </div>
            ))
          )}
        </div>
      )}

      {/* Message list */}
      <div ref={scrollRef} style={{ flex: 1, overflowY: "auto", padding: "16px" }}>
        {items.length === 0 && !currentResponse && (
          <div style={{ textAlign: "center", color: "#94a3b8", marginTop: "40px", fontSize: "14px" }}>
            开始聊天吧~
          </div>
        )}

        {items.map((item, i) => {
          if (item.type === "user") {
            return (
              <div key={i} style={{ marginBottom: "12px", display: "flex", justifyContent: "flex-end" }}>
                <div style={bubbleStyle("user")}>{item.content}</div>
              </div>
            );
          }
          if (item.type === "assistant") {
            if (!item.content.trim()) return null;
            return (
              <div key={i} style={{ marginBottom: "12px", display: "flex", justifyContent: "flex-start" }}>
                <div style={bubbleStyle("assistant")}>{item.content}</div>
              </div>
            );
          }
          if (item.type === "tool") {
            return (
              <div key={i} style={{ marginBottom: "12px", maxWidth: "85%" }}>
                {item.toolCalls?.map((tc, j) => (
                  <ToolCallBlock key={j} name={tc.name} arguments={tc.arguments} result={tc.result} />
                ))}
              </div>
            );
          }
          if (item.type === "error") {
            return (
              <div key={i} style={{ marginBottom: "12px", display: "flex", justifyContent: "flex-start" }}>
                <div style={{ ...bubbleStyle("assistant"), background: "#fef2f2", color: "#dc2626" }}>
                  {item.content}
                </div>
              </div>
            );
          }
          return null;
        })}

        {/* Live tool calls */}
        {currentToolCalls.length > 0 && (
          <div style={{ marginBottom: "12px", maxWidth: "85%" }}>
            {currentToolCalls.map((tc, j) => (
              <ToolCallBlock key={j} name={tc.name} arguments={tc.arguments} result={tc.result} isRunning={tc.isRunning} />
            ))}
          </div>
        )}

        {/* Streaming response */}
        {currentResponse && (
          <div style={{ marginBottom: "12px", display: "flex", justifyContent: "flex-start" }}>
            <div style={bubbleStyle("assistant")}>
              {currentResponse}
              <span style={{ animation: "blink 1s infinite" }}>▌</span>
            </div>
          </div>
        )}
      </div>

      {/* Input bar */}
      <form
        onSubmit={handleSubmit}
        style={{
          display: "flex",
          gap: "8px",
          padding: "12px 16px",
          borderTop: "1px solid #e2e8f0",
          background: "#fff",
        }}
      >
        <input
          value={input}
          onChange={(e) => setInput(e.target.value)}
          placeholder="输入消息..."
          style={{
            flex: 1,
            padding: "10px 14px",
            borderRadius: "10px",
            border: "1px solid #e2e8f0",
            fontSize: "14px",
            outline: "none",
            color: "#1e293b",
          }}
        />
        <button
          type="submit"
          disabled={isLoading}
          style={{
            padding: "10px 20px",
            borderRadius: "10px",
            border: "none",
            background: isLoading ? "#cbd5e1" : "#0ea5e9",
            color: "#fff",
            fontSize: "14px",
            fontWeight: 500,
            cursor: isLoading ? "default" : "pointer",
          }}
        >
          {isLoading ? "..." : "发送"}
        </button>
      </form>
    </div>
  );
}

/* ---------- Styles ---------- */

function bubbleStyle(role: "user" | "assistant"): React.CSSProperties {
  return {
    maxWidth: "80%",
    padding: "10px 14px",
    borderRadius: role === "user" ? "16px 16px 4px 16px" : "16px 16px 16px 4px",
    background: role === "user" ? "#0ea5e9" : "#fff",
    color: role === "user" ? "#fff" : "#1e293b",
    fontSize: "14px",
    lineHeight: "1.6",
    boxShadow: "0 1px 3px rgba(0,0,0,0.08)",
    wordBreak: "break-word",
    whiteSpace: "pre-wrap",
  };
}

const sessionBarStyle: React.CSSProperties = {
  display: "flex",
  alignItems: "center",
  gap: "8px",
  padding: "8px 16px",
  borderBottom: "1px solid #e2e8f0",
  background: "#fff",
  flexShrink: 0,
};

const newSessionBtnStyle: React.CSSProperties = {
  padding: "4px 12px",
  borderRadius: "6px",
  border: "1px solid #e2e8f0",
  background: "#f8fafc",
  color: "#0ea5e9",
  fontSize: "12px",
  fontWeight: 600,
  cursor: "pointer",
  whiteSpace: "nowrap",
};

const sessionDropdownStyle: React.CSSProperties = {
  maxHeight: "240px",
  overflowY: "auto",
  borderBottom: "1px solid #e2e8f0",
  background: "#fff",
  flexShrink: 0,
};
