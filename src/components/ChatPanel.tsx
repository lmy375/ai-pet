import { useState, type FormEvent } from "react";

interface Props {
  onSend: (message: string) => void;
  isLoading: boolean;
}

export function ChatPanel({ onSend, isLoading }: Props) {
  const [input, setInput] = useState("");

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault();
    if (!input.trim() || isLoading) return;
    onSend(input.trim());
    setInput("");
  };

  return (
    <form
      onSubmit={handleSubmit}
      onMouseDown={(e) => e.stopPropagation()}
      style={{
        position: "absolute",
        bottom: "12px",
        left: "12px",
        right: "12px",
        display: "flex",
        gap: "8px",
        zIndex: 10,
      }}
    >
      <input
        value={input}
        onChange={(e) => setInput(e.target.value)}
        placeholder="说点什么..."
        style={{
          flex: 1,
          padding: "8px 14px",
          borderRadius: "20px",
          border: "1px solid rgba(200,200,200,0.6)",
          background: "rgba(255,255,255,0.88)",
          backdropFilter: "blur(8px)",
          fontSize: "14px",
          outline: "none",
          color: "#333",
        }}
      />
      <button
        type="submit"
        disabled={isLoading}
        style={{
          padding: "8px 16px",
          borderRadius: "20px",
          border: "none",
          background: isLoading ? "#ccc" : "#0ea5e9",
          color: "white",
          cursor: isLoading ? "default" : "pointer",
          fontSize: "14px",
          fontWeight: 500,
          transition: "background 0.2s",
        }}
      >
        {isLoading ? "..." : "发送"}
      </button>
    </form>
  );
}
