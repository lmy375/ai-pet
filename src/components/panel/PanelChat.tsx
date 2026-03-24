import { useState, useRef, useEffect, useCallback } from "react";
import { invoke, Channel } from "@tauri-apps/api/core";

interface ChatMessage {
  role: "user" | "assistant" | "system";
  content: string;
}

type StreamEvent =
  | { event: "chunk"; data: { text: string } }
  | { event: "done"; data: Record<string, never> }
  | { event: "error"; data: { message: string } };

export function PanelChat() {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState("");
  const [isLoading, setIsLoading] = useState(false);
  const [currentResponse, setCurrentResponse] = useState("");
  const scrollRef = useRef<HTMLDivElement>(null);
  const soulRef = useRef("");

  // Load soul on mount
  useEffect(() => {
    invoke<string>("get_soul").then((soul) => {
      soulRef.current = soul;
      setMessages([{ role: "system", content: soul }]);
    });
  }, []);

  // Auto-scroll
  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [messages, currentResponse]);

  const sendMessage = useCallback(async (content: string) => {
    const userMsg: ChatMessage = { role: "user", content };
    const updated = [...messages, userMsg];
    setMessages(updated);
    setIsLoading(true);
    setCurrentResponse("");

    const onEvent = new Channel<StreamEvent>();
    let accumulated = "";

    onEvent.onmessage = (event: StreamEvent) => {
      if (event.event === "chunk") {
        accumulated += event.data.text;
        setCurrentResponse(accumulated);
      } else if (event.event === "done") {
        setMessages((prev) => [...prev, { role: "assistant", content: accumulated }]);
        setCurrentResponse("");
        setIsLoading(false);
      } else if (event.event === "error") {
        setMessages((prev) => [...prev, { role: "assistant", content: `错误: ${event.data.message}` }]);
        setCurrentResponse("");
        setIsLoading(false);
      }
    };

    try {
      await invoke("chat", { messages: updated, onEvent });
    } catch (err) {
      setMessages((prev) => [...prev, { role: "assistant", content: `错误: ${err}` }]);
      setIsLoading(false);
    }
  }, [messages]);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!input.trim() || isLoading) return;
    sendMessage(input.trim());
    setInput("");
  };

  const visibleMessages = messages.filter((m) => m.role !== "system");

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%" }}>
      {/* Message list */}
      <div ref={scrollRef} style={{ flex: 1, overflowY: "auto", padding: "16px" }}>
        {visibleMessages.length === 0 && !currentResponse && (
          <div style={{ textAlign: "center", color: "#94a3b8", marginTop: "40px", fontSize: "14px" }}>
            开始聊天吧~
          </div>
        )}
        {visibleMessages.map((msg, i) => (
          <div key={i} style={{ marginBottom: "12px", display: "flex", justifyContent: msg.role === "user" ? "flex-end" : "flex-start" }}>
            <div
              style={{
                maxWidth: "80%",
                padding: "10px 14px",
                borderRadius: msg.role === "user" ? "16px 16px 4px 16px" : "16px 16px 16px 4px",
                background: msg.role === "user" ? "#0ea5e9" : "#fff",
                color: msg.role === "user" ? "#fff" : "#1e293b",
                fontSize: "14px",
                lineHeight: "1.6",
                boxShadow: "0 1px 3px rgba(0,0,0,0.08)",
                wordBreak: "break-word",
                whiteSpace: "pre-wrap",
              }}
            >
              {msg.content}
            </div>
          </div>
        ))}
        {currentResponse && (
          <div style={{ marginBottom: "12px", display: "flex", justifyContent: "flex-start" }}>
            <div
              style={{
                maxWidth: "80%",
                padding: "10px 14px",
                borderRadius: "16px 16px 16px 4px",
                background: "#fff",
                color: "#1e293b",
                fontSize: "14px",
                lineHeight: "1.6",
                boxShadow: "0 1px 3px rgba(0,0,0,0.08)",
                wordBreak: "break-word",
                whiteSpace: "pre-wrap",
              }}
            >
              {currentResponse}
              <span style={{ animation: "blink 1s infinite" }}>▌</span>
            </div>
          </div>
        )}
      </div>

      {/* Input bar */}
      <form onSubmit={handleSubmit} style={{ display: "flex", gap: "8px", padding: "12px 16px", borderTop: "1px solid #e2e8f0", background: "#fff" }}>
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
