import { useEffect, useRef } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";

const SAVE_DEBOUNCE = 500; // ms idle after a drag before persisting ball position
const DRAG_THRESHOLD = 5; // px of travel before a press becomes a drag

// The floating ball the pet collapses into: a small always-on-top circle with
// the pet's avatar. Drag to move it (position is remembered), click to expand
// the pet. Expansion goes through the backend (expand_from_ball), which hides
// this window and shows the pet at its previous position.
export function BallApp() {
  const press = useRef<{ x: number; y: number } | null>(null);
  const dragging = useRef(false);

  useEffect(() => {
    const win = getCurrentWindow();
    let saveTimer: ReturnType<typeof setTimeout> | null = null;
    let unlisten: (() => void) | null = null;
    let cancelled = false;

    // Remember where the user dragged the ball so the next collapse reappears
    // there (debounced like the pet window's own position save).
    const setup = async () => {
      const off = await win.onMoved(({ payload }) => {
        if (saveTimer) clearTimeout(saveTimer);
        saveTimer = setTimeout(() => {
          invoke("save_ball_position", { x: payload.x, y: payload.y }).catch((e) =>
            console.error("Failed to save ball position:", e),
          );
        }, SAVE_DEBOUNCE);
      });
      if (cancelled) {
        off();
        return;
      }
      unlisten = off;
    };
    setup();

    return () => {
      cancelled = true;
      if (saveTimer) clearTimeout(saveTimer);
      unlisten?.();
    };
  }, []);

  // Press-and-move drags via the native window drag; a press released without
  // moving counts as a click and expands the pet. We can't call startDragging
  // on mousedown — a native drag session swallows the mouseup, so the click
  // would never fire — instead we wait for real movement first.
  const handleMouseDown = (e: React.MouseEvent) => {
    e.preventDefault();
    press.current = { x: e.clientX, y: e.clientY };
    dragging.current = false;
  };

  const handleMouseMove = (e: React.MouseEvent) => {
    const start = press.current;
    if (!start || dragging.current) return;
    if (Math.hypot(e.clientX - start.x, e.clientY - start.y) > DRAG_THRESHOLD) {
      dragging.current = true;
      getCurrentWindow().startDragging();
    }
  };

  const handleMouseUp = () => {
    if (!press.current) return;
    press.current = null;
    if (dragging.current) return; // native drag moved the ball, not a click
    invoke("expand_from_ball").catch((e) =>
      console.error("Failed to expand from ball:", e),
    );
  };

  return (
    <div
      onMouseDown={handleMouseDown}
      onMouseMove={handleMouseMove}
      onMouseUp={handleMouseUp}
      className="ball-pop flex h-screen w-full cursor-pointer items-center justify-center"
    >
      {/* The ball: pet avatar in a white-rimmed circle with a soft edge shadow
          (Doubao-style) so it reads against light wallpapers. The window is 8px
          larger than the ball on each side — that margin is exactly where the
          shadow lives; a shadow clipped by the window edge looks like a gray
          ring, so the ball must stay smaller than the window. Hover keeps the
          scale subtle (105) for the same reason. */}
      <div className="relative h-[44px] w-[44px] overflow-hidden rounded-full border-2 border-white bg-surface shadow-[0_2px_8px_rgba(20,30,55,0.28),0_0_0_1px_rgba(20,30,55,0.06)] transition-transform duration-150 hover:scale-105">
          <img
            src="/pet/idle.png"
            alt="pet"
            draggable={false}
            className="builtin-pet h-full w-full object-cover"
          />
      </div>
    </div>
  );
}

export default BallApp;
