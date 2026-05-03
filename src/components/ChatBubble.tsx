interface Props {
  message: string;
  visible: boolean;
  onClick?: () => void;
}

export function ChatBubble({ message, visible, onClick }: Props) {
  if (!visible || !message) return null;

  return (
    <div
      onClick={onClick}
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
        cursor: onClick ? "pointer" : "default",
      }}
    >
      {/* Iter R24: subtle dismiss affordance — the bubble was already
          click-to-dismiss in R1b, but with no visual hint that clicking
          would do anything. The ✕ corner icon makes it discoverable.
          Click bubbles up to the parent div's onClick, so clicking either
          the ✕ or the bubble body is equivalent (no separate handler).
          Tooltip on the ✕ explains the strength-of-signal nuance. */}
      {onClick && (
        <span
          aria-label="dismiss bubble"
          title="点掉气泡（5 秒内点 = 给宠物 '别这条' 信号；R1b dismissed feedback）"
          style={{
            position: "absolute",
            top: "4px",
            right: "8px",
            fontSize: "11px",
            color: "#94a3b8",
            opacity: 0.55,
            userSelect: "none",
          }}
        >
          ✕
        </span>
      )}
      {message}
    </div>
  );
}
