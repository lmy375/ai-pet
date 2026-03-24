interface Props {
  message: string;
  visible: boolean;
}

export function ChatBubble({ message, visible }: Props) {
  if (!visible || !message) return null;

  return (
    <div
      style={{
        position: "absolute",
        bottom: "100px",
        left: "12px",
        right: "12px",
        maxHeight: "80px",
        overflowY: "auto",
        padding: "10px 14px",
        background: "#ffffff",
        borderRadius: "16px",
        boxShadow: "none",
        border: "1px solid #bae6fd",
        fontSize: "13px",
        lineHeight: "1.5",
        color: "#333",
        zIndex: 10,
        wordBreak: "break-word",
      }}
    >
      {message}
    </div>
  );
}
