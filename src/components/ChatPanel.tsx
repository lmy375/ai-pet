import { useState, useRef, useEffect } from "react";

interface Props {
  onSend: (message: string) => void;
  isLoading: boolean;
}

const PANEL_STYLES = `
.pet-chat-input:focus {
  border-color: #38bdf8;
  box-shadow: 0 0 0 2px rgba(56,189,248,0.18);
}
`;

/// 桌面宠物输入框。作为 flex column 里的第三段、永远紧贴底部。**不再使用
/// position:absolute** —— 既往多次出现 absolute-bottom 与 ChatMini 重叠的
/// bug，本组件保持普通 flex item，由 App 容器通过 flex column 自然堆叠
/// (Live2D / ChatMini / ChatPanel) 即可。
export function ChatPanel({ onSend, isLoading }: Props) {
  const [input, setInput] = useState("");
  const textareaRef = useRef<HTMLTextAreaElement>(null);

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
    <>
      <style>{PANEL_STYLES}</style>
      <div
        onMouseDown={(e) => e.stopPropagation()}
        style={{
          padding: "8px 12px 12px",
          flexShrink: 0,
          display: "flex",
          alignItems: "center",
          gap: "8px",
        }}
      >
        <textarea
          ref={textareaRef}
          className="pet-chat-input"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder={isLoading ? "宠物正在回复中..." : "说点什么..."}
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
            transition: "border-color 150ms ease-out, box-shadow 150ms ease-out",
          }}
        />
      </div>
    </>
  );
}
