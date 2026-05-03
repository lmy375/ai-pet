interface Props {
  message: string;
  visible: boolean;
  onClick?: () => void;
}

// Iter R40: CSS keyframes for the bubble's mount-time fade-in. Runs once
// per visible→mounted transition (parent toggles `visible` so each new
// utterance gets its own fade-in). Inline <style> keeps the animation
// scoped to this file without needing a global stylesheet.
//
// translateY(4px) → 0 gives a faint "settle" feel — bubble doesn't pop
// in flat. 220ms is short enough to feel responsive; longer lags the
// utterance vs reading the text.
//
// Iter R41: also adds a `:active` press feedback so the click that
// triggers dismiss has a tactile reaction (scale 0.97 for 80ms). Without
// it, click → bubble disappears with no transition felt → user wonders
// "did the click register?". The press scale is the universal "I felt
// your tap" affordance from native UI.
const BUBBLE_STYLES = `
@keyframes pet-bubble-fade-in {
  from { opacity: 0; transform: translateY(4px); }
  to   { opacity: 1; transform: translateY(0); }
}
.pet-bubble:active {
  transform: scale(0.97);
}
`;

export function ChatBubble({ message, visible, onClick }: Props) {
  if (!visible || !message) return null;

  return (
    <>
      <style>{BUBBLE_STYLES}</style>
      <div
        className="pet-bubble"
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
          animation: "pet-bubble-fade-in 220ms ease-out",
          transition: "transform 80ms ease-out",
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
    </>
  );
}
