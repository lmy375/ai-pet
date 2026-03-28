import { useState, useRef, useEffect } from "react";

interface Props {
  onSend: (message: string) => void;
  isLoading: boolean;
  onOpenPanel?: () => void;
}

export function ChatPanel({ onSend, isLoading, onOpenPanel }: Props) {
  const [input, setInput] = useState("");
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  // Auto-resize textarea height
  useEffect(() => {
    const el = textareaRef.current;
    if (el) {
      el.style.height = "auto";
      el.style.height = Math.min(el.scrollHeight, 80) + "px";
    }
  }, [input]);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      if (!input.trim() || isLoading) return;
      onSend(input.trim());
      setInput("");
    }
  };

  return (
    <div
      onMouseDown={(e) => e.stopPropagation()}
      style={{
        position: "absolute",
        bottom: "12px",
        left: "12px",
        right: "12px",
        display: "flex",
        alignItems: "center",
        gap: "8px",
        zIndex: 10,
      }}
    >
      <textarea
        ref={textareaRef}
        value={input}
        onChange={(e) => setInput(e.target.value)}
        onKeyDown={handleKeyDown}
        placeholder="说点什么..."
        rows={1}
        style={{
          flex: 1,
          padding: "9px 14px",
          borderRadius: "20px",
          border: "1px solid rgba(200,200,200,0.5)",
          background: "rgba(255,255,255,0.9)",
          backdropFilter: "blur(8px)",
          fontSize: "14px",
          outline: "none",
          color: "#333",
          resize: "none",
          lineHeight: "1.4",
          fontFamily: "inherit",
          overflow: "hidden",
          boxSizing: "border-box",
        }}
      />
      {onOpenPanel && (
        <div
          onClick={onOpenPanel}
          style={{
            width: "36px",
            height: "36px",
            borderRadius: "50%",
            background: "rgba(255,255,255,0.9)",
            backdropFilter: "blur(8px)",
            border: "1px solid rgba(200,200,200,0.5)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            cursor: "pointer",
            fontSize: "15px",
            flexShrink: 0,
            opacity: 0.7,
            transition: "opacity 0.2s",
            boxSizing: "border-box",
          }}
          title="打开设置面板"
          onMouseEnter={(e) => (e.currentTarget.style.opacity = "1")}
          onMouseLeave={(e) => (e.currentTarget.style.opacity = "0.7")}
        >
          ⚙
        </div>
      )}
    </div>
  );
}
