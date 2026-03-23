import { useRef, useCallback } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Live2DCharacter } from "./components/Live2DCharacter";
import { ChatBubble } from "./components/ChatBubble";
import { ChatPanel } from "./components/ChatPanel";
import { useChat } from "./hooks/useChat";

function App() {
  const { isLoading, sendMessage, displayMessage, showBubble } = useChat();
  const modelRef = useRef<any>(null);

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

  return (
    <div
      onMouseDown={handleDrag}
      style={{
        width: "100%",
        height: "100vh",
        position: "relative",
        background: "transparent",
        userSelect: "none",
      }}
    >
      <ChatBubble message={displayMessage} visible={showBubble} />
      <Live2DCharacter
        modelPath="/models/miku/miku.model3.json"
        onModelReady={handleModelReady}
      />
      <ChatPanel onSend={handleSend} isLoading={isLoading} />
    </div>
  );
}

export default App;
