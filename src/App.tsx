import { useRef, useCallback } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { Live2DCharacter } from "./components/Live2DCharacter";
import { ChatBubble } from "./components/ChatBubble";
import { ChatPanel } from "./components/ChatPanel";
import { useChat } from "./hooks/useChat";
import { useAutoHide } from "./hooks/useAutoHide";
import { useSettings } from "./hooks/useSettings";

function App() {
  const { settings, soul, loaded } = useSettings();
  const { isLoading, sendMessage, displayMessage, showBubble } = useChat(soul);
  const modelRef = useRef<any>(null);
  const { hidden, handleMouseEnter } = useAutoHide();

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

      {/* Panel button (gear) */}
      {!hidden && (
        <div
          onMouseDown={(e) => e.stopPropagation()}
          onClick={(e) => {
            e.stopPropagation();
            openPanel();
          }}
          style={{
            position: "absolute",
            top: "8px",
            right: "8px",
            width: "24px",
            height: "24px",
            borderRadius: "50%",
            background: "rgba(255,255,255,0.7)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            cursor: "pointer",
            zIndex: 20,
            fontSize: "14px",
            backdropFilter: "blur(4px)",
          }}
        >
          ⚙
        </div>
      )}

      <ChatBubble message={displayMessage} visible={showBubble && !hidden} />
      <Live2DCharacter
        key={settings.live_2d_model_path}
        modelPath={settings.live_2d_model_path}
        onModelReady={handleModelReady}
      />
      {!hidden && <ChatPanel onSend={handleSend} isLoading={isLoading} />}
    </div>
  );
}

export default App;
