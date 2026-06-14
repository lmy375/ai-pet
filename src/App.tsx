import { useRef, useCallback } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { Live2DCharacter } from "./components/Live2DCharacter";
import { ChatThread } from "./components/ChatThread";
import { ChatInput } from "./components/ChatInput";
import { GearIcon, ChevronRight } from "./components/Icons";
import { useChat } from "./hooks/useChat";
import { useAutoHide } from "./hooks/useAutoHide";
import { useSettings } from "./hooks/useSettings";

function App() {
  const { settings, loaded } = useSettings();
  const { items, currentResponse, currentToolCalls, isLoading, sendMessage } = useChat();
  const modelRef = useRef<any>(null);
  const { hidden, handleMouseEnter } = useAutoHide();

  const handleModelReady = useCallback((model: any) => {
    modelRef.current = model;
  }, []);

  const handleSend = useCallback((msg: string) => sendMessage(msg), [sendMessage]);

  const handleDrag = (e: React.MouseEvent) => {
    const tag = (e.target as HTMLElement).tagName;
    if (tag === "INPUT" || tag === "BUTTON" || tag === "TEXTAREA") return;
    e.preventDefault();
    getCurrentWindow().startDragging();
  };

  const handleResize = (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    getCurrentWindow().startResizeDragging("SouthEast");
  };

  const openPanel = () => {
    invoke("open_panel").catch(console.error);
  };

  if (!loaded) return null;

  return (
    <div
      onMouseDown={handleDrag}
      onMouseEnter={handleMouseEnter}
      className="relative flex h-screen w-full flex-col overflow-hidden bg-transparent select-none"
    >
      {/* Tab indicator — visible strip when auto-hidden at the screen edge.
          Hovering it triggers the root onMouseEnter → slideBack to expand. */}
      {hidden && (
        <div className="absolute left-0 top-1/2 z-50 flex h-[52px] w-5 -translate-y-1/2 cursor-pointer items-center justify-center rounded-l-xl bg-accent shadow-lg">
          <ChevronRight className="h-4 w-4 rotate-180 text-white" />
        </div>
      )}

      {/* Pet character with breathing — always mounted so it survives auto-hide
          (unmounting would tear down and fail to re-init the Live2D/PIXI canvas).
          shrink-0 keeps it at its natural size at the top, never covered by chat. */}
      <div className="animate-breath pointer-events-none mx-auto w-[300px] shrink-0">
        <Live2DCharacter
          key={settings.live_2d_model_path}
          modelPath={settings.live_2d_model_path}
          onModelReady={handleModelReady}
        />
      </div>

      {!hidden && (
        <>
          {/* Settings — top-right, aligned with the chat window's right edge */}
          <button
            onClick={openPanel}
            title="打开设置面板"
            className="absolute right-2 top-2 z-20 flex h-9 w-9 items-center justify-center rounded-xl border border-slate-300/60 bg-white/80 text-slate-600 backdrop-blur-md transition-colors hover:bg-white"
          >
            <GearIcon className="h-5 w-5" />
          </button>

          {/* Chat thread — same component & logic as the panel, only the
              container styling differs. Fills the space below the character. */}
          <div
            onMouseDown={(e) => e.stopPropagation()}
            className="z-10 min-h-0 flex-1 px-2"
          >
            <ChatThread
              items={items}
              currentToolCalls={currentToolCalls}
              streaming={currentResponse}
              loading={isLoading}
              className="h-full rounded-2xl border border-sky-200/60 bg-white/45 px-3 py-3 backdrop-blur-md"
            />
          </div>

          {/* Input bar */}
          <div className="z-10 shrink-0 px-2 pb-3 pt-2">
            <ChatInput onSend={handleSend} isLoading={isLoading} />
          </div>

          {/* Resize grip — drag to freely resize the window (and the chat area) */}
          <div
            onMouseDown={handleResize}
            title="拖动调整大小"
            className="absolute bottom-0 right-0 z-30 h-4 w-4 cursor-nwse-resize"
          >
            <div className="absolute bottom-1 right-1 h-2 w-2 border-b-2 border-r-2 border-slate-400/70" />
          </div>
        </>
      )}
    </div>
  );
}

export default App;
