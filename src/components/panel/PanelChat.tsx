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

export function PanelChat() {
  const [items, setItems] = useState<ChatItem[]>([]);
  const [input, setInput] = useState("");
  const [isLoading, setIsLoading] = useState(false);
  const [currentResponse, setCurrentResponse] = useState("");
  const [currentToolCalls, setCurrentToolCalls] = useState<ToolCall[]>([]);
  const scrollRef = useRef<HTMLDivElement>(null);
  const soulRef = useRef("");
  const messagesRef = useRef<any[]>([]);

  useEffect(() => {
    invoke<string>("get_soul").then((soul) => {
      soulRef.current = soul;
      messagesRef.current = [{ role: "system", content: soul }];
    });
  }, []);

  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [items, currentResponse, currentToolCalls]);

  const sendMessage = useCallback(
    async (content: string) => {
      const userMsg = { role: "user", content };
      messagesRef.current = [...messagesRef.current, userMsg];
      setItems((prev) => [...prev, { type: "user", content }]);
      setIsLoading(true);
      setCurrentResponse("");
      setCurrentToolCalls([]);

      const onEvent = new Channel<StreamEvent>();
      let accumulated = "";
      let toolCalls: ToolCall[] = [];

      // Helper: flush completed tool calls to persistent items
      const flushToolCalls = () => {
        if (toolCalls.length > 0) {
          const snapshot = [...toolCalls];
          setItems((prev) => [...prev, { type: "tool" as const, content: "", toolCalls: snapshot }]);
          toolCalls = [];
          setCurrentToolCalls([]);
        }
      };

      onEvent.onmessage = (event: StreamEvent) => {
        if (event.event === "chunk") {
          // Flush any completed tool calls before new text
          flushToolCalls();
          accumulated += event.data.text;
          setCurrentResponse(accumulated);
        } else if (event.event === "toolStart") {
          // Flush accumulated text before tool calls (skip if whitespace-only)
          if (accumulated.trim()) {
            setItems((prev) => [...prev, { type: "assistant", content: accumulated }]);
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
          // Flush any pending tool calls
          flushToolCalls();
          // Flush final text (skip if whitespace-only)
          if (accumulated.trim()) {
            setItems((prev) => [...prev, { type: "assistant", content: accumulated }]);
            messagesRef.current = [
              ...messagesRef.current,
              { role: "assistant", content: accumulated },
            ];
          }
          setCurrentResponse("");
          setIsLoading(false);
        } else if (event.event === "error") {
          setItems((prev) => [
            ...prev,
            { type: "error", content: event.data.message },
          ]);
          setCurrentResponse("");
          setCurrentToolCalls([]);
          setIsLoading(false);
        }
      };

      try {
        await invoke("chat", { messages: messagesRef.current, onEvent });
      } catch (err) {
        setItems((prev) => [...prev, { type: "error", content: `${err}` }]);
        setIsLoading(false);
      }
    },
    [],
  );

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!input.trim() || isLoading) return;
    sendMessage(input.trim());
    setInput("");
  };

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%" }}>
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
                  ❌ {item.content}
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
