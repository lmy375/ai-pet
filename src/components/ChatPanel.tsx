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
  // Iter R52 / R54 / R58: 🔇 mute button. Left-click toggles 30 min
  // default (R52 fast-path); right-click opens preset menu with
  // 15/30/60/120 min options + clear (R54 flexible-path).
  //
  // R58: refresh mute state on each user interaction (entry-point
  // refresh codified in R57 IDEA). Without this, after auto-expiry
  // frontend `muted` stays true and button still shows red — matching
  // the same stale-state bug R57 fixed for note popover. Behavior:
  //   - left-click: refetch fresh state, toggle based on it (not on
  //     potentially-stale local state)
  //   - right-click: refetch fresh state before showing menu so menu
  //     reflects current backend
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
  // R58: returns the freshly-fetched mute state so callers can use the
  // truth without depending on the React-state update latency. Errors
  // fall back to current React state.
  const refreshMuteState = async (): Promise<boolean> => {
    try {
      const iso = await invoke<string>("get_mute_until");
      const isMuted = iso !== "";
      setMuted(isMuted);
      return isMuted;
    } catch {
      return muted;
    }
  };
  const handleMuteClick = async () => {
    const isMuted = await refreshMuteState();
    applyMute(isMuted ? 0 : 30);
  };
  const handleMuteContextMenu = async (e: React.MouseEvent) => {
    e.preventDefault();
    if (!showMenu) {
      await refreshMuteState();
    }
    setShowMenu((v) => !v);
  };
  // Close menu when clicking anywhere outside it.
  useEffect(() => {
    if (!showMenu) return;
    const close = () => setShowMenu(false);
    window.addEventListener("click", close);
    return () => window.removeEventListener("click", close);
  }, [showMenu]);
  // Iter R55: transient instruction note. Lets user leave a context note
  // ("I'm in a meeting" / "I'm not feeling well") that pet's next
  // proactive prompt sees as "[临时指示]" directive. Different from mute
  // (R52) which fully blocks — note keeps pet active but with awareness.
  const [showNotePopover, setShowNotePopover] = useState(false);
  const [noteText, setNoteText] = useState("");
  const [noteMinutes, setNoteMinutes] = useState(60);
  const [noteActive, setNoteActive] = useState(false);
  useEffect(() => {
    invoke<[string, string]>("get_transient_note")
      .then(([text]) => {
        setNoteActive(text !== "");
        if (text) setNoteText(text);
      })
      .catch(() => setNoteActive(false));
  }, []);
  // Iter R57: refresh popover state on each open. Without this, an
  // auto-expired note still shows its stale text and noteActive=true.
  // Behavior:
  //   - if backend has active note → load text into textarea + mark active
  //   - if backend has no note → mark inactive but **don't wipe noteText**
  //     so a user-typed draft survives close→reopen (preserve draft on
  //     unsaved popover dismissal)
  // Closes when popover is already open (toggle).
  const handleNoteToggle = async () => {
    if (showNotePopover) {
      setShowNotePopover(false);
      return;
    }
    try {
      const [text] = await invoke<[string, string]>("get_transient_note");
      if (text) {
        setNoteText(text);
        setNoteActive(true);
      } else {
        setNoteActive(false);
        // Don't clear noteText — preserve in-progress draft.
      }
    } catch (e) {
      console.error("get_transient_note failed:", e);
    }
    setShowNotePopover(true);
  };
  const handleNoteSubmit = async () => {
    try {
      const result = await invoke<string>("set_transient_note", {
        text: noteText,
        minutes: noteMinutes,
      });
      setNoteActive(result !== "");
      setShowNotePopover(false);
    } catch (e) {
      console.error("set_transient_note failed:", e);
    }
  };
  const handleNoteClear = async () => {
    try {
      await invoke<string>("set_transient_note", { text: "", minutes: 0 });
      setNoteActive(false);
      setNoteText("");
      setShowNotePopover(false);
    } catch (e) {
      console.error("set_transient_note clear failed:", e);
    }
  };
  useEffect(() => {
    if (!showNotePopover) return;
    const close = (e: MouseEvent) => {
      // Only close on outside clicks. Inside-popover clicks have stopPropagation.
      if (!(e.target as HTMLElement)?.closest?.(".pet-note-popover")) {
        setShowNotePopover(false);
      }
    };
    window.addEventListener("click", close);
    return () => window.removeEventListener("click", close);
  }, [showNotePopover]);

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
        {/* Iter R55: 📝 note button — leave a transient context for pet. */}
        <div style={{ position: "relative" }}>
          <div
            className="pet-settings-btn"
            onClick={(e) => {
              e.stopPropagation();
              handleNoteToggle();
            }}
            style={{
              width: "36px",
              height: "36px",
              borderRadius: "50%",
              background: noteActive ? "rgba(8,145,178,0.9)" : "rgba(255,255,255,0.9)",
              backdropFilter: "blur(8px)",
              border: noteActive ? "1px solid rgba(8,145,178,0.5)" : "1px solid rgba(200,200,200,0.5)",
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              cursor: "pointer",
              fontSize: "15px",
              flexShrink: 0,
              boxSizing: "border-box",
              color: noteActive ? "#fff" : "inherit",
            }}
            title={noteActive ? "已留临时指示 — 点击编辑或解除" : "给 pet 留临时指示（如「在开会」「身体不太舒服」）"}
          >
            📝
          </div>
          {showNotePopover && (
            <div
              className="pet-note-popover"
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
                padding: "10px",
                width: "260px",
                zIndex: 100,
                display: "flex",
                flexDirection: "column",
                gap: "8px",
              }}
            >
              <div style={{ fontSize: "11px", color: "#64748b" }}>
                给 pet 留临时指示（自动到期清除）
              </div>
              <textarea
                value={noteText}
                onChange={(e) => setNoteText(e.target.value)}
                placeholder="例如：我在开会到 14:00 / 身体不太舒服请轻一点"
                rows={3}
                className="pet-chat-input"
                style={{
                  width: "100%",
                  padding: "6px 8px",
                  borderRadius: "6px",
                  border: "1px solid #e2e8f0",
                  fontSize: "12px",
                  resize: "vertical",
                  fontFamily: "inherit",
                  outline: "none",
                  boxSizing: "border-box",
                  transition: "border-color 150ms ease-out, box-shadow 150ms ease-out",
                }}
              />
              <div style={{ display: "flex", gap: "4px", flexWrap: "wrap" }}>
                {[30, 60, 120, 240].map((m) => (
                  <button
                    key={m}
                    type="button"
                    onClick={() => setNoteMinutes(m)}
                    style={{
                      padding: "3px 8px",
                      fontSize: "11px",
                      borderRadius: "6px",
                      border: `1px solid ${noteMinutes === m ? "#0891b2" : "#cbd5e1"}`,
                      background: noteMinutes === m ? "#0891b2" : "#fff",
                      color: noteMinutes === m ? "#fff" : "#475569",
                      cursor: "pointer",
                      fontFamily: "inherit",
                      fontWeight: 600,
                    }}
                  >
                    {m} min
                  </button>
                ))}
              </div>
              <div style={{ display: "flex", gap: "6px" }}>
                <button
                  type="button"
                  onClick={handleNoteSubmit}
                  disabled={!noteText.trim()}
                  style={{
                    flex: 1,
                    padding: "6px",
                    borderRadius: "6px",
                    border: "none",
                    background: noteText.trim() ? "#0891b2" : "#cbd5e1",
                    color: "#fff",
                    cursor: noteText.trim() ? "pointer" : "not-allowed",
                    fontFamily: "inherit",
                    fontWeight: 600,
                    fontSize: "12px",
                  }}
                >
                  保存 · {noteMinutes} min
                </button>
                {noteActive && (
                  <button
                    type="button"
                    onClick={handleNoteClear}
                    style={{
                      padding: "6px 10px",
                      borderRadius: "6px",
                      border: "1px solid #dc2626",
                      background: "#fff",
                      color: "#dc2626",
                      cursor: "pointer",
                      fontFamily: "inherit",
                      fontWeight: 600,
                      fontSize: "12px",
                    }}
                  >
                    解除
                  </button>
                )}
              </div>
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
