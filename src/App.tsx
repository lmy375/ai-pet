import { useRef, useCallback, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { Live2DCharacter } from "./components/Live2DCharacter";
import { GallerySlideshow } from "./components/GallerySlideshow";
import { ChatThread } from "./components/ChatThread";
import { ChatInput } from "./components/ChatInput";
import { GearIcon, ChevronRight, ChevronDown, PinIcon } from "./components/Icons";
import { useChat } from "./hooks/useChat";
import { useAutoHide } from "./hooks/useAutoHide";
import { useSettings } from "./hooks/useSettings";

function App() {
  const { settings, loaded } = useSettings();
  const { items, currentResponse, currentToolCalls, isLoading, sendMessage } = useChat();
  const modelRef = useRef<any>(null);
  const { hidden, handleMouseEnter, pauseTimer, resumeTimer } = useAutoHide();
  const [pinned, setPinned] = useState(false);
  const [chatCollapsed, setChatCollapsed] = useState(false);

  const galleryOn = settings.gallery_enabled && !!settings.gallery_dir;

  // Pin: keep the pet pinned above every window and stop it auto-hiding (handy
  // for watching the gallery slideshow). Unpin restores auto-hide.
  const togglePin = useCallback(() => {
    setPinned((prev) => {
      const next = !prev;
      getCurrentWindow().setAlwaysOnTop(next).catch(console.error);
      next ? pauseTimer() : resumeTimer();
      return next;
    });
  }, [pauseTimer, resumeTimer]);

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

      {/* Main visual: either the gallery slideshow or the Live2D character.
          Gallery mode fills the window so the slideshow is prominent; the pet
          stays a fixed-size block at the top. The Live2D canvas is always
          mounted (when not in gallery mode) so it survives auto-hide —
          unmounting would tear down and fail to re-init the PIXI canvas. */}
      {galleryOn ? (
        <div className="min-h-0 flex-1 p-2">
          <GallerySlideshow dir={settings.gallery_dir} intervalSec={settings.gallery_interval} />
        </div>
      ) : (
        <div className="animate-breath pointer-events-none mx-auto w-[300px] shrink-0">
          <Live2DCharacter
            key={settings.live_2d_model_path}
            modelPath={settings.live_2d_model_path}
            onModelReady={handleModelReady}
          />
        </div>
      )}

      {!hidden && (
        <>
          {/* Pin toggle — top-left. Pinned = stay above all windows + no auto-hide. */}
          <button
            onClick={togglePin}
            title={pinned ? "取消钉住" : "钉住（保持置顶、不自动收起）"}
            className={`absolute left-2 top-2 z-20 flex h-9 w-9 items-center justify-center rounded-xl border backdrop-blur-md transition-colors ${
              pinned
                ? "border-accent bg-accent text-white"
                : "border-slate-300/60 bg-white/80 text-slate-600 hover:bg-white"
            }`}
          >
            <PinIcon className="h-5 w-5" />
          </button>

          {/* Settings — top-right, aligned with the chat window's right edge */}
          <button
            onClick={openPanel}
            title="打开设置面板"
            className="absolute right-2 top-2 z-20 flex h-9 w-9 items-center justify-center rounded-xl border border-slate-300/60 bg-white/80 text-slate-600 backdrop-blur-md transition-colors hover:bg-white"
          >
            <GearIcon className="h-5 w-5" />
          </button>

          {/* Chat thread — collapsible. When collapsed only the pet/gallery (and
              the bottom toggle) remain. Same component & logic as the panel; in
              gallery mode the slideshow sits above it at fixed height. */}
          {!chatCollapsed && (
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
          )}

          {/* Bottom bar: collapse toggle sits right beside the chat box. The
              toggle stays put when collapsed (so the chat can be reopened); the
              input only renders while expanded. */}
          <div
            onMouseDown={(e) => e.stopPropagation()}
            className="z-10 flex shrink-0 items-end gap-1.5 px-2 pb-3 pt-2"
          >
            <button
              onClick={() => setChatCollapsed((v) => !v)}
              title={chatCollapsed ? "展开聊天" : "收起聊天"}
              className="flex h-9 w-9 shrink-0 items-center justify-center rounded-xl border border-slate-300/60 bg-white/80 text-slate-600 backdrop-blur-md transition-colors hover:bg-white"
            >
              <ChevronDown className={`h-5 w-5 transition-transform ${chatCollapsed ? "" : "rotate-180"}`} />
            </button>
            {!chatCollapsed && (
              <div className="flex-1">
                <ChatInput onSend={handleSend} isLoading={isLoading} />
              </div>
            )}
          </div>

          {/* Resize grip — drag to freely resize the window (and the chat area) */}
          {!chatCollapsed && (
            <div
              onMouseDown={handleResize}
              title="拖动调整大小"
              className="absolute bottom-0 right-0 z-30 h-4 w-4 cursor-nwse-resize"
            >
              <div className="absolute bottom-1 right-1 h-2 w-2 border-b-2 border-r-2 border-slate-400/70" />
            </div>
          )}
        </>
      )}
    </div>
  );
}

export default App;
