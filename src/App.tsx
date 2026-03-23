import { useRef, useCallback } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Live2DCharacter } from "./components/Live2DCharacter";
import { ChatBubble } from "./components/ChatBubble";
import { ChatPanel } from "./components/ChatPanel";
import { useChat } from "./hooks/useChat";
import { useAutoHide } from "./hooks/useAutoHide";

function App() {
  const { isLoading, sendMessage, displayMessage, showBubble } = useChat();
  const modelRef = useRef<any>(null);
  const { minimized, handleActivity } = useAutoHide();
  const dragInfo = useRef({ x: 0, y: 0, time: 0 });

  const handleModelReady = useCallback((model: any) => {
    modelRef.current = model;
  }, []);

  const handleSend = useCallback(
    (msg: string) => {
      handleActivity();
      sendMessage(msg);
    },
    [sendMessage, handleActivity],
  );

  const handleDrag = (e: React.MouseEvent) => {
    const tag = (e.target as HTMLElement).tagName;
    if (tag === "INPUT" || tag === "BUTTON" || tag === "TEXTAREA") return;
    e.preventDefault();
    dragInfo.current = { x: e.screenX, y: e.screenY, time: Date.now() };
    getCurrentWindow().startDragging();
  };

  const handleClick = (e: React.MouseEvent) => {
    if (!minimized) return;
    const d = dragInfo.current;
    const dx = Math.abs(e.screenX - d.x);
    const dy = Math.abs(e.screenY - d.y);
    const dt = Date.now() - d.time;
    if (dx < 5 && dy < 5 && dt < 300) {
      handleActivity();
    }
  };

  return (
    <div
      onMouseDown={handleDrag}
      onMouseMove={minimized ? undefined : handleActivity}
      onClick={handleClick}
      style={{
        width: "100%",
        height: "100vh",
        background: "transparent",
        userSelect: "none",
        position: "relative",
      }}
    >
      {/* Minimized: show bubble at bottom-right corner */}
      {minimized && (
        <div
          style={{
            position: "absolute",
            bottom: "8px",
            right: "8px",
            width: "32px",
            height: "32px",
            borderRadius: "50%",
            background:
              "radial-gradient(circle at 35% 35%, #c4b5fd, #7c4dff 60%, #5b21b6)",
            boxShadow:
              "0 0 10px rgba(124,77,255,0.5), 0 0 3px rgba(196,181,253,0.8) inset",
            cursor: "pointer",
            animation: "pulse 2s ease-in-out infinite",
          }}
        />
      )}
      <style>{`
        @keyframes pulse {
          0%, 100% { transform: scale(1); opacity: 0.9; }
          50% { transform: scale(1.08); opacity: 1; }
        }
      `}</style>

      {/* Full view - always mounted, just hidden with opacity */}
      <div
        style={{
          opacity: minimized ? 0 : 1,
          pointerEvents: minimized ? "none" : "auto",
          transition: "opacity 0.2s ease",
          width: "100%",
          height: "100%",
          position: "absolute",
          top: 0,
          left: 0,
        }}
      >
        <ChatBubble message={displayMessage} visible={showBubble} />
        <Live2DCharacter
          modelPath="/models/miku/miku.model3.json"
          onModelReady={handleModelReady}
        />
        <ChatPanel onSend={handleSend} isLoading={isLoading} />
      </div>
    </div>
  );
}

export default App;
