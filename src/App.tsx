import { useCallback, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { Live2DCharacter } from "./components/Live2DCharacter";
import { GallerySlideshow } from "./components/GallerySlideshow";
import { ChatThread } from "./components/ChatThread";
import { ChatInput } from "./components/ChatInput";
import { ExternalLinkIcon, ChevronRight, ChevronDown, PinIcon } from "./components/Icons";
import { FloatingIconButton } from "./components/ui/IconButton";
import { useChat } from "./hooks/useChat";
import { useAutoHide } from "./hooks/useAutoHide";
import { useSettings } from "./hooks/useSettings";
import { useI18n } from "./i18n";

function App() {
  const { settings, loaded } = useSettings();
  const { t } = useI18n();
  const { items, currentResponse, currentToolCalls, isLoading, sendMessage } = useChat();
  const { hidden, handleMouseEnter, pauseTimer, resumeTimer } = useAutoHide();
  const [pinned, setPinned] = useState(false);
  const [chatCollapsed, setChatCollapsed] = useState(false);
  // Corner marks fade out when the cursor leaves the window and become solid
  // while it's over the pet. Driven by explicit enter/leave state (reliable on
  // this transparent, borderless window) rather than CSS :hover.
  const [hovered, setHovered] = useState(false);

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

  const handleSend = useCallback(
    (msg: string, images?: string[]) => sendMessage(msg, images),
    [sendMessage],
  );

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
      onMouseEnter={() => {
        handleMouseEnter();
        setHovered(true);
      }}
      onMouseLeave={() => setHovered(false)}
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
        // Top padding reserves a strip for the pin / open icons so the media
        // never sits under them (the buttons float at top-2, h-9).
        <div className="min-h-0 flex-1 px-2 pb-2 pt-12">
          <GallerySlideshow dir={settings.gallery_dir} intervalSec={settings.gallery_interval} />
        </div>
      ) : (
        <div className="animate-breath pointer-events-none mx-auto w-[300px] shrink-0">
          <Live2DCharacter
            key={settings.live_2d_model_path}
            modelPath={settings.live_2d_model_path}
          />
        </div>
      )}

      {!hidden && (
        <>
          {/* Right-angle corner marks — a subtle frame so the otherwise
              transparent, borderless window reads as a grabbable surface. The
              drop-shadow gives a thin dark halo so the gray stays visible on both
              light and dark wallpapers. Purely decorative: pointer-events-none
              lets mousedown fall through to the root's handleDrag, so clicking
              anywhere (corners included) drags. The bottom-right one doubles as
              the visual for the resize grip below. */}
          <div
            className={`pointer-events-none absolute inset-0 z-0 transition-opacity duration-300 [filter:drop-shadow(0_0_1px_rgba(0,0,0,0.55))] ${
              hovered ? "opacity-100" : "opacity-25"
            }`}
          >
            <span className="absolute left-0 top-0 h-3 w-3 rounded-tl-md border-l-2 border-t-2 border-slate-300/90" />
            <span className="absolute right-0 top-0 h-3 w-3 rounded-tr-md border-r-2 border-t-2 border-slate-300/90" />
            <span className="absolute bottom-0 left-0 h-3 w-3 rounded-bl-md border-b-2 border-l-2 border-slate-300/90" />
            {/* Bottom-right is the "busy" corner: when expanded it holds the send
                button + resize grip, so we skip the mark there to avoid crowding.
                Shown only when collapsed (no send button then). */}
            {chatCollapsed && (
              <span className="absolute bottom-0 right-0 h-3 w-3 rounded-br-md border-b-2 border-r-2 border-slate-300/90" />
            )}
          </div>

          {/* Pin toggle — top-left. Pinned = stay above all windows + no auto-hide. */}
          <FloatingIconButton
            active={pinned}
            onClick={togglePin}
            title={pinned ? t("app.pin.on") : t("app.pin.off")}
            className="absolute left-2 top-2 z-20"
          >
            <PinIcon className="h-5 w-5" />
          </FloatingIconButton>

          {/* Settings — top-right, aligned with the chat window's right edge */}
          <FloatingIconButton
            onClick={openPanel}
            title={t("app.openSettings")}
            className="absolute right-2 top-2 z-20"
          >
            <ExternalLinkIcon className="h-5 w-5" />
          </FloatingIconButton>

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
            className="z-10 flex shrink-0 items-end gap-1.5 px-3 pb-3.5 pt-2"
          >
            <FloatingIconButton
              onClick={() => setChatCollapsed((v) => !v)}
              title={chatCollapsed ? t("app.chat.expand") : t("app.chat.collapse")}
              className="shrink-0"
            >
              <ChevronDown className={`h-5 w-5 transition-transform ${chatCollapsed ? "" : "rotate-180"}`} />
            </FloatingIconButton>
            {!chatCollapsed && (
              <div className="flex-1">
                <ChatInput onSend={handleSend} isLoading={isLoading} />
              </div>
            )}
          </div>

          {/* Resize grip — drag to freely resize the window. Transparent hit area
              only; the bottom-right corner mark above is its visual. */}
          {!chatCollapsed && (
            <div
              onMouseDown={handleResize}
              title={t("app.resize")}
              className="absolute bottom-0 right-0 z-30 h-4 w-4 cursor-nwse-resize"
            />
          )}
        </>
      )}
    </div>
  );
}

export default App;
