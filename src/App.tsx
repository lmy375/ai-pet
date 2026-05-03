import { useEffect, useRef, useState, useCallback } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
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
      {/* Tab indicator — visible strip when hidden */}
      {hidden && (
        <div
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
          }}
        >
          <div
            style={{
              width: "0",
              height: "0",
              borderTop: "6px solid transparent",
              borderBottom: "6px solid transparent",
              borderRight: "6px solid rgba(255,255,255,0.8)",
            }}
          />
        </div>
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
