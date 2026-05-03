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
.pet-mute-menu-item {
  background: transparent;
  border: none;
  padding: 6px 10px;
  font-size: 12px;
  text-align: left;
  cursor: pointer;
  border-radius: 6px;
  font-family: inherit;
  transition: background 100ms ease-out;
}
.pet-mute-menu-item:hover {
  background: #f1f5f9;
}
`;

export function ChatPanel({ onSend, isLoading, onOpenPanel }: Props) {
  const [input, setInput] = useState("");
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  // Iter R52 / R54: 🔇 mute button. Left-click toggles 30 min default
  // (R52 fast-path); right-click opens preset menu with 15/30/60/120
  // min options + clear (R54 flexible-path). Two paths cover the two
  // user types — quick mute users and granular-control users.
  const [muted, setMuted] = useState(false);
  const [showMenu, setShowMenu] = useState(false);
  useEffect(() => {
    // Initial probe so the button starts in correct state on mount.
    invoke<string>("get_mute_until")
      .then((iso) => setMuted(iso !== ""))
      .catch(() => setMuted(false));
  }, []);
  const applyMute = async (minutes: number) => {
    try {
      const result = await invoke<string>("set_mute_minutes", { minutes });
      setMuted(result !== "");
    } catch (e) {
      console.error("set_mute_minutes failed:", e);
    }
    setShowMenu(false);
  };
  const handleMuteClick = () => applyMute(muted ? 0 : 30);
  const handleMuteContextMenu = (e: React.MouseEvent) => {
    e.preventDefault();
    setShowMenu((v) => !v);
  };
  // Close menu when clicking anywhere outside it.
  useEffect(() => {
    if (!showMenu) return;
    const close = () => setShowMenu(false);
    window.addEventListener("click", close);
    return () => window.removeEventListener("click", close);
  }, [showMenu]);

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
        <div style={{ position: "relative" }}>
          <div
            className="pet-settings-btn"
            onClick={handleMuteClick}
            onContextMenu={handleMuteContextMenu}
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
            title={muted ? "宠物已静音 — 点击解除（右键选时长）" : "左键 30 分钟静音 / 右键选时长（仅跳 proactive，reactive 不影响）"}
          >
            🔇
          </div>
          {/* Iter R54: preset menu opened via right-click. Click on item
              applies, then closes; outside click also closes. */}
          {showMenu && (
            <div
              onClick={(e) => e.stopPropagation()}
              style={{
                position: "absolute",
                bottom: "44px",
                right: 0,
                background: "rgba(255,255,255,0.98)",
                backdropFilter: "blur(8px)",
                border: "1px solid rgba(200,200,200,0.5)",
                borderRadius: "10px",
                boxShadow: "0 4px 12px rgba(0,0,0,0.1)",
                padding: "4px",
                display: "flex",
                flexDirection: "column",
                gap: "1px",
                minWidth: "120px",
                zIndex: 100,
              }}
            >
              {[
                { label: "静音 15 分钟", minutes: 15 },
                { label: "静音 30 分钟", minutes: 30 },
                { label: "静音 60 分钟", minutes: 60 },
                { label: "静音 120 分钟", minutes: 120 },
                { label: "解除静音", minutes: 0 },
              ].map((opt) => (
                <button
                  key={opt.minutes}
                  type="button"
                  className="pet-mute-menu-item"
                  onClick={() => applyMute(opt.minutes)}
                  style={{
                    color: opt.minutes === 0 ? "#dc2626" : "#1e293b",
                  }}
                >
                  {opt.label}
                </button>
              ))}
            </div>
          )}
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
