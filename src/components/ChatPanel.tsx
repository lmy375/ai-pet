import { useState, useRef, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

interface Props {
  onSend: (message: string) => void;
  isLoading: boolean;
  onOpenPanel?: () => void;
}

// Iter R46: extracted CSS so ⚙ button uses :hover (R41 codified
// pattern "CSS pseudo-class > React state for pure visual states")
// and the textarea gets a real focus ring (its `outline: none`
// previously stripped the browser default with no replacement —
// accessibility hole).
//
// Iter R48: adds "AI is thinking" pulsing dots indicator that
// appears when isLoading. Three dots staggered via animation-delay
// so the pulse cascades — industry-standard "thinking" visual.
const PANEL_STYLES = `
.pet-settings-btn {
  opacity: 0.7;
  transition: opacity 200ms ease-out;
}
.pet-settings-btn:hover {
  opacity: 1;
}
.pet-chat-input:focus {
  border-color: #38bdf8;
  box-shadow: 0 0 0 2px rgba(56,189,248,0.18);
}
@keyframes pet-loading-dot-pulse {
  0%, 100% { opacity: 0.25; transform: translateY(0); }
  50%      { opacity: 1; transform: translateY(-2px); }
}
.pet-loading-dot {
  width: 5px;
  height: 5px;
  border-radius: 50%;
  background: #38bdf8;
  animation: pet-loading-dot-pulse 1.2s ease-in-out infinite;
}
.pet-loading-dot:nth-child(2) {
  animation-delay: 0.18s;
}
.pet-loading-dot:nth-child(3) {
  animation-delay: 0.36s;
}
`;

export function ChatPanel({ onSend, isLoading, onOpenPanel }: Props) {
  const [input, setInput] = useState("");
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  // Iter R52: 🔇 mute button cycles through quick presets. Polls
  // get_mute_until on click so display reflects backend truth (not stale
  // local state). Cycle: not muted → 30 min → 60 min → cleared. Each
  // click invokes set_mute_minutes; backend returns ISO timestamp or
  // empty string for "cleared".
  const [muted, setMuted] = useState(false);
  useEffect(() => {
    // Initial probe so the button starts in correct state on mount.
    invoke<string>("get_mute_until")
      .then((iso) => setMuted(iso !== ""))
      .catch(() => setMuted(false));
  }, []);
  const handleMuteClick = async () => {
    try {
      // Toggle: if currently muted → clear; else set 30 min.
      const minutes = muted ? 0 : 30;
      const result = await invoke<string>("set_mute_minutes", { minutes });
      setMuted(result !== "");
    } catch (e) {
      console.error("set_mute_minutes failed:", e);
    }
  };

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
    <>
      <style>{PANEL_STYLES}</style>
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
          className="pet-chat-input"
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
            transition: "border-color 150ms ease-out, box-shadow 150ms ease-out",
          }}
        />
        {/* Iter R52: 🔇 mute toggle button between textarea and ⚙. Click
            to mute pet for 30 min (skips proactive gate); click again to
            unmute. Visible state via emoji + opacity (muted = 1.0 + red
            tint; unmuted = 0.7 + neutral). */}
        <div
          className="pet-settings-btn"
          onClick={handleMuteClick}
          style={{
            width: "36px",
            height: "36px",
            borderRadius: "50%",
            background: muted ? "rgba(220,38,38,0.9)" : "rgba(255,255,255,0.9)",
            backdropFilter: "blur(8px)",
            border: muted ? "1px solid rgba(220,38,38,0.5)" : "1px solid rgba(200,200,200,0.5)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            cursor: "pointer",
            fontSize: "15px",
            flexShrink: 0,
            boxSizing: "border-box",
            color: muted ? "#fff" : "inherit",
          }}
          title={muted ? "宠物已静音 30 分钟 — 点击解除" : "静音宠物 30 分钟（仅跳过 proactive，reactive chat 不影响）"}
        >
          🔇
        </div>
        {/* Iter R48: AI-thinking pulsing dots when isLoading. Sits between
            textarea and ⚙ button so it doesn't fight either's space. */}
        {isLoading && (
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: "4px",
              padding: "0 6px",
            }}
            title="宠物正在回复中..."
          >
            <div className="pet-loading-dot" />
            <div className="pet-loading-dot" />
            <div className="pet-loading-dot" />
          </div>
        )}
        {onOpenPanel && (
          <div
            className="pet-settings-btn"
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
              boxSizing: "border-box",
            }}
            title="打开设置面板"
          >
            ⚙
          </div>
        )}
      </div>
    </>
  );
}
