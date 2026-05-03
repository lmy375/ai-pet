import { useEffect, useRef, useState, useCallback } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Live2DCharacter } from "./components/Live2DCharacter";
import { ChatBubble } from "./components/ChatBubble";
import { ChatPanel } from "./components/ChatPanel";
import { useChat } from "./hooks/useChat";
import { useAutoHide } from "./hooks/useAutoHide";
import { useSettings } from "./hooks/useSettings";
import { useMoodAnimation } from "./hooks/useMoodAnimation";

function App() {
  const { settings, soul, loaded } = useSettings();
  const { isLoading, sendMessage, displayMessage, showBubble } = useChat(soul);
  const modelRef = useRef<any>(null);
  const { hidden, handleMouseEnter } = useAutoHide();
  useMoodAnimation(modelRef);

  // Iter F1: bubble auto-dismiss after 60s of being visible. Without this the
  // desktop bubble stays showing the last assistant message forever — proactive
  // utterances at 9am stuck on screen all day. 60s is enough to read; if the
  // user wants the message back they can open the chat panel for full history.
  // Loading bubbles (mid-stream) and the message arrival reset the timer.
  //
  // Iter R1b: track when the current bubble first appeared so a click within
  // the QUICK_DISMISS_MS window records an active-rejection feedback signal
  // (distinct from passive ignore). Click after the window still hides the
  // bubble but doesn't pollute feedback history with late hides.
  const QUICK_DISMISS_MS = 5000;
  const [bubbleDismissed, setBubbleDismissed] = useState(false);
  const bubbleShownAt = useRef<number | null>(null);
  useEffect(() => {
    setBubbleDismissed(false);
    if (!showBubble || !displayMessage || isLoading) {
      bubbleShownAt.current = null;
      return;
    }
    bubbleShownAt.current = Date.now();
    const t = setTimeout(() => setBubbleDismissed(true), 60_000);
    return () => clearTimeout(t);
  }, [displayMessage, showBubble, isLoading]);

  // Iter R45: count proactive messages that arrived while pet is auto-hidden
  // (bubble suppressed via `visible={... && !hidden && ...}`). Tab indicator
  // renders a badge when count > 0 so user sees "pet has unread things to
  // say". Resets when hidden flips false (user mouse-entered → pet returned
  // → bubble can now show next message normally).
  //
  // Why a ref + setState pair: the listener inside useEffect captures
  // `hidden` only at mount; using a ref lets the listener always read the
  // latest value without re-subscribing on every hidden flip.
  const hiddenRef = useRef(hidden);
  useEffect(() => {
    hiddenRef.current = hidden;
  }, [hidden]);
  const [unreadWhileHidden, setUnreadWhileHidden] = useState(0);
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    (async () => {
      unlisten = await listen("proactive-message", () => {
        if (hiddenRef.current) {
          setUnreadWhileHidden((n) => n + 1);
        }
      });
    })();
    return () => {
      if (unlisten) unlisten();
    };
  }, []);
  // Clear badge when pet un-hides (user is now seeing pet again).
  useEffect(() => {
    if (!hidden) setUnreadWhileHidden(0);
  }, [hidden]);

  const handleBubbleClick = useCallback(() => {
    const shownAt = bubbleShownAt.current;
    setBubbleDismissed(true);
    if (shownAt && Date.now() - shownAt < QUICK_DISMISS_MS && displayMessage) {
      invoke("record_bubble_dismissed", { excerpt: displayMessage }).catch(
        console.error,
      );
    }
  }, [displayMessage]);

  const handleModelReady = useCallback((model: any) => {
    modelRef.current = model;
  }, []);

  const handleSend = useCallback(
    (msg: string) => {
      sendMessage(msg);
    },
    [sendMessage],
  );

  const handleDrag = (e: React.MouseEvent) => {
    const tag = (e.target as HTMLElement).tagName;
    if (tag === "INPUT" || tag === "BUTTON" || tag === "TEXTAREA") return;
    e.preventDefault();
    getCurrentWindow().startDragging();
  };

  const openPanel = () => {
    invoke("open_panel").catch(console.error);
  };

  if (!loaded) return null;

  return (
    <div
      onMouseDown={handleDrag}
      onMouseEnter={handleMouseEnter}
      style={{
        width: "100%",
        height: "100vh",
        background: "transparent",
        userSelect: "none",
        position: "relative",
        overflow: "hidden",
      }}
    >
      {/* Tab indicator — visible strip when hidden.
          Iter R43: slide-in animation when transitioning to hidden state +
          hover widen affordance. Mirrors ChatBubble's interaction state
          machine (R40+R41+R42) — entrance animation + hover state for
          "I am here, click me to bring pet back". */}
      {hidden && (
        <>
          <style>{`
            @keyframes pet-tab-slide-in {
              from { left: -16px; opacity: 0; }
              to   { left: 0; opacity: 1; }
            }
            @keyframes pet-tab-arrow-bob {
              0%, 100% { transform: translateX(0); }
              50%      { transform: translateX(-2px); }
            }
            .pet-tab:hover {
              width: 22px;
            }
            .pet-tab-arrow {
              animation: pet-tab-arrow-bob 1.6s ease-in-out infinite;
            }
            .pet-tab:hover .pet-tab-arrow {
              animation-play-state: paused;
            }
          `}</style>
          <div
            className="pet-tab"
            style={{
              position: "absolute",
              left: 0,
              top: "50%",
              transform: "translateY(-50%)",
              width: "16px",
              height: "50px",
              background: "linear-gradient(180deg, #7dd3fc 0%, #38bdf8 50%, #0ea5e9 100%)",
              borderRadius: "10px 0 0 10px",
              boxShadow: "-2px 0 8px rgba(56,189,248,0.3)",
              zIndex: 50,
              cursor: "pointer",
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              animation: "pet-tab-slide-in 280ms ease-out",
              transition: "width 120ms ease-out",
            }}
          >
            <div
              className="pet-tab-arrow"
              style={{
                width: "0",
                height: "0",
                borderTop: "6px solid transparent",
                borderBottom: "6px solid transparent",
                borderRight: "6px solid rgba(255,255,255,0.8)",
              }}
            />
            {/* Iter R45: unread badge — appears when pet spoke ≥1 time while
                auto-hidden. Position top-right of tab so it doesn't fight
                with the centered arrow. Number capped at 9+ so single
                badge stays small at very chatty days. */}
            {unreadWhileHidden > 0 && (
              <div
                style={{
                  position: "absolute",
                  top: "-4px",
                  right: "-4px",
                  minWidth: "14px",
                  height: "14px",
                  padding: "0 3px",
                  background: "#dc2626",
                  color: "#fff",
                  fontSize: "10px",
                  fontWeight: 700,
                  borderRadius: "7px",
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                  border: "1.5px solid #fff",
                  boxShadow: "0 1px 3px rgba(0,0,0,0.2)",
                }}
                title={`pet 在隐藏期间主动开口了 ${unreadWhileHidden} 次（mouse-enter 让 pet 回来后会自动消失）`}
              >
                {unreadWhileHidden > 9 ? "9+" : unreadWhileHidden}
              </div>
            )}
          </div>
        </>
      )}

      <ChatBubble
        message={displayMessage}
        visible={showBubble && !hidden && !bubbleDismissed}
        onClick={handleBubbleClick}
      />
      <Live2DCharacter
        key={settings.live_2d_model_path}
        modelPath={settings.live_2d_model_path}
        onModelReady={handleModelReady}
      />
      {!hidden && <ChatPanel onSend={handleSend} isLoading={isLoading} onOpenPanel={openPanel} />}
    </div>
  );
}

export default App;
