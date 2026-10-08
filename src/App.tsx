import { useCallback, useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { LogicalSize } from "@tauri-apps/api/dpi";
import { invoke } from "@tauri-apps/api/core";
import { Live2DCharacter } from "./components/Live2DCharacter";
import { GallerySlideshow } from "./components/GallerySlideshow";
import { AnimatedPet, type Emotion } from "./components/AnimatedPet";
import { ChatThread } from "./components/ChatThread";
import { ChatInput } from "./components/ChatInput";
import {
  ExternalLinkIcon,
  ChevronDown,
  MinimizeIcon,
  PinIcon,
} from "./components/Icons";
import { FloatingIconButton } from "./components/ui/IconButton";
import { useChat } from "./hooks/useChat";
import { useAutoHide } from "./hooks/useAutoHide";
import { useSettings } from "./hooks/useSettings";
import { useTauriEvent } from "./hooks/useTauriEvent";
import { useI18n } from "./i18n";

// Breathing room under the pet once the window shrinks to it, so the bottom
// corner marks still read as a frame.
const COLLAPSED_PAD = 8;

function App() {
  const { settings, loaded } = useSettings();
  const { t } = useI18n();
  const {
    items,
    currentResponse,
    currentReasoning,
    currentToolCalls,
    isLoading,
    turnStartedAt,
    turnTokens,
    sendMessage,
    editMessage,
    stopStreaming,
  } = useChat();
  const { hidden, handleMouseEnter, pauseTimer, resumeTimer, collapseToBall } = useAutoHide();
  const [pinned, setPinned] = useState(false);
  const [chatCollapsed, setChatCollapsed] = useState(false);
  const petBlockRef = useRef<HTMLDivElement>(null);
  const expandedHeightRef = useRef<number | null>(null);
  // Corner marks fade out when the cursor leaves the window and become solid
  // while it's over the pet. Driven by explicit enter/leave state (reliable on
  // this transparent, borderless window) rather than CSS :hover.
  const [hovered, setHovered] = useState(false);

  // Which visual fills the window, from the settings' explicit three-way
  // choice. Gallery additionally needs a directory; Live2D needs both paths —
  // the settings UI gates the choice, but hand-edited config can still name a
  // kind with nothing configured, so each branch re-checks and falls through to
  // the built-in image pet rather than showing a blank window.
  const galleryOn = settings.pet_kind === "gallery" && !!settings.gallery_dir;
  const live2dOn =
    settings.pet_kind === "live2d" &&
    !!settings.live_2d_model_path &&
    !!settings.live_2d_core_path;

  // Built-in pet emotion, derived purely from chat state (no backend / LLM tag):
  // idle by default → thinking while a turn streams → happy for a couple of
  // seconds the moment a reply finishes → back to idle. Tracks the previous
  // loading flag so a happy burst only fires on the true→false edge (not on
  // launch). The Live2D/gallery modes ignore this; it only feeds AnimatedPet.
  const [emotion, setEmotion] = useState<Emotion>("idle");
  const prevLoadingRef = useRef(isLoading);
  const happyTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(() => {
    const wasLoading = prevLoadingRef.current;
    prevLoadingRef.current = isLoading;
    if (isLoading) {
      if (happyTimerRef.current) {
        clearTimeout(happyTimerRef.current);
        happyTimerRef.current = null;
      }
      setEmotion("thinking");
    } else if (wasLoading) {
      setEmotion("happy");
      if (happyTimerRef.current) clearTimeout(happyTimerRef.current);
      happyTimerRef.current = setTimeout(() => setEmotion("idle"), 2000);
    }
    return () => {
      if (happyTimerRef.current) {
        clearTimeout(happyTimerRef.current);
      }
    };
  }, [isLoading]);

  // The main window is hidden while the panel is open (open_panel) — and that's
  // where Live2D settings are edited. A Live2D canvas built while the window is
  // hidden can never get a working WebGL context, so rather than rebuilding it
  // offscreen (which leaves a blank canvas when it reappears) we unmount the
  // canvas entirely while hidden and remount it once the window is shown again.
  // Visibility is driven by the backend (main-hidden / main-shown) instead of
  // the focus event, which doesn't reliably fire on hide. See CLAUDE.md.
  const [windowVisible, setWindowVisible] = useState(true);
  // One-shot pop-in replayed whenever the window reappears — from the ball or
  // from the panel closing — so the pet eases back in instead of blinking.
  const [reappearing, setReappearing] = useState(false);
  useTauriEvent("main-hidden", () => setWindowVisible(false));
  useTauriEvent("main-shown", () => {
    setWindowVisible(true);
    setReappearing(true);
    // Double rAF: let the hidden frame paint first, then transition in.
    requestAnimationFrame(() => requestAnimationFrame(() => setReappearing(false)));
  });

  // Pin: keep the pet pinned above every window and stop it auto-hiding (handy
  // for watching the gallery slideshow). Unpin restores auto-hide.
  const applyPin = useCallback(
    (next: boolean) => {
      setPinned(next);
      getCurrentWindow().setAlwaysOnTop(next).catch(console.error);
      next ? pauseTimer() : resumeTimer();
    },
    [pauseTimer, resumeTimer],
  );

  // Hide on demand: the same collapse the idle timer runs. Pinning suppresses
  // it, so unpin first (synchronously) instead of leaving a dead button.
  const hidePet = useCallback(() => {
    if (pinned) applyPin(false);
    collapseToBall();
  }, [pinned, applyPin, collapseToBall]);

  // Collapsing pulls the window's bottom edge up under the pet instead of
  // leaving dead space; expanding restores the height the window had. setSize
  // keeps the top-left anchored, so the pet never moves. Gallery mode is
  // excluded: the slideshow is sized to fill whatever height it's given, so it
  // has no collapsed height to shrink to.
  useEffect(() => {
    if (galleryOn) return;
    const win = getCurrentWindow();
    const apply = async () => {
      const { width, height } = (await win.innerSize()).toLogical(await win.scaleFactor());
      if (chatCollapsed) {
        const block = petBlockRef.current;
        if (!block) return;
        expandedHeightRef.current = height;
        // offsetTop/Height, not getBoundingClientRect: the block carries the
        // breathing transform, and its bob would leak into the window height.
        const collapsed = block.offsetTop + block.offsetHeight + COLLAPSED_PAD;
        await win.setSize(new LogicalSize(width, collapsed));
      } else {
        const restored = expandedHeightRef.current;
        expandedHeightRef.current = null;
        if (restored !== null) await win.setSize(new LogicalSize(width, restored));
      }
    };
    apply().catch(console.error);
  }, [chatCollapsed, galleryOn]);

  const handleSend = useCallback(
    (msg: string, images?: string[]) => sendMessage(msg, images),
    [sendMessage],
  );

  const handleDrag = (e: React.MouseEvent) => {
    // An icon is an <svg> child of its button, so match the closest interactive
    // ancestor: a mousedown on the glyph must click the button, not drag the
    // window out from under it.
    if ((e.target as Element).closest("button, input, textarea")) return;
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
      className={`relative flex h-screen w-full flex-col overflow-hidden bg-transparent select-none transition-all duration-300 ease-out ${
        hidden
          ? "scale-50 opacity-0" // collapse: shrink toward where the ball pops in
          : reappearing
            ? "scale-75 opacity-0" // just re-shown: start small, transition in
            : "scale-100 opacity-100"
      }`}
    >
      {/* Main visual: either the gallery slideshow or the Live2D character.
          Gallery mode fills the window so the slideshow is prominent; the pet
          stays a fixed-size block at the top. The Live2D canvas is always
          mounted (when not in gallery mode) so it survives auto-hide —
          unmounting would tear down and fail to re-init the PIXI canvas. */}
      {galleryOn ? (
        // Top padding reserves a strip for the pin / hide / open icons so the
        // media never sits under them (the buttons float at top-2, size-6).
        <div className="min-h-0 flex-1 px-2 pb-2 pt-9">
          <GallerySlideshow dir={settings.gallery_dir} intervalSec={settings.gallery_interval} />
        </div>
      ) : live2dOn ? (
        <div ref={petBlockRef} className="animate-breath pointer-events-none mx-auto w-[300px] shrink-0">
          {windowVisible && (
            <Live2DCharacter
              key={`${settings.live_2d_core_path}|${settings.live_2d_model_path}`}
              modelPath={settings.live_2d_model_path}
              corePath={settings.live_2d_core_path}
            />
          )}
        </div>
      ) : (
        // Image pet: the raster character — bundled `public/pet` art by default,
        // or the user-chosen `pet_image_dir` when set (per-emotion files with a
        // bundled fallback, see AnimatedPet). Emotions come from chat state:
        // idle → thinking while a turn streams → happy when it finishes.
        <div ref={petBlockRef} className="animate-breath pointer-events-none mx-auto w-[300px] shrink-0">
          <AnimatedPet emotion={emotion} dir={settings.pet_image_dir} />
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
            <span className="absolute left-0 top-0 h-3 w-3 rounded-tl-md border-l-2 border-t-2 border-line/90" />
            <span className="absolute right-0 top-0 h-3 w-3 rounded-tr-md border-r-2 border-t-2 border-line/90" />
            <span className="absolute bottom-0 left-0 h-3 w-3 rounded-bl-md border-b-2 border-l-2 border-line/90" />
            <span className="absolute bottom-0 right-0 h-3 w-3 rounded-br-md border-b-2 border-r-2 border-line/90" />
          </div>

          {/* Pin toggle — top-left. Pinned = stay above all windows + no auto-hide. */}
          <FloatingIconButton
            active={pinned}
            onClick={() => applyPin(!pinned)}
            title={pinned ? t("app.pin.on") : t("app.pin.off")}
            className="absolute left-2 top-2 z-20"
          >
            <PinIcon />
          </FloatingIconButton>

          {/* Top-right cluster — collapse the chat, hide the pet, open the
              panel. Aligned with the chat window's right edge. */}
          <div
            onMouseDown={(e) => e.stopPropagation()}
            className="absolute right-2 top-2 z-20 flex items-center gap-1.5"
          >
            <FloatingIconButton
              onClick={() => setChatCollapsed((v) => !v)}
              title={chatCollapsed ? t("app.chat.expand") : t("app.chat.collapse")}
            >
              <ChevronDown className={`transition-transform ${chatCollapsed ? "" : "rotate-180"}`} />
            </FloatingIconButton>
            <FloatingIconButton onClick={hidePet} title={t("app.hidePet")}>
              <MinimizeIcon />
            </FloatingIconButton>
            <FloatingIconButton onClick={openPanel} title={t("app.openSettings")}>
              <ExternalLinkIcon />
            </FloatingIconButton>
          </div>

          {/* Chat thread — collapsible. When collapsed only the pet/gallery
              remains. Same component & logic as the panel; in gallery mode the
              slideshow sits above it at fixed height. */}
          {!chatCollapsed && (
            <div
              onMouseDown={(e) => e.stopPropagation()}
              className="z-10 min-h-0 flex-1 px-2 pt-2"
            >
              <ChatThread
                items={items}
                currentToolCalls={currentToolCalls}
                streaming={currentResponse}
                streamingReasoning={currentReasoning}
                loading={isLoading}
                turnStartedAt={turnStartedAt}
                turnTokens={turnTokens}
                className="h-full rounded-card border border-line bg-surface/55 px-3 py-3 backdrop-blur-md"
                onEditMessage={editMessage}
              />
            </div>
          )}

          {/* Bottom bar: the input, only while expanded. */}
          {!chatCollapsed && (
            <div
              onMouseDown={(e) => e.stopPropagation()}
              className="z-10 shrink-0 px-3 pb-3.5 pt-2"
            >
              <ChatInput onSend={handleSend} isLoading={isLoading} onStop={stopStreaming} />
            </div>
          )}

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
