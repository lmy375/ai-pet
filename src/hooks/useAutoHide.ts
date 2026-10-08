import { useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";

const BLUR_TIMEOUT = 3000; // 3s after blur → collapse into the ball
const SAVE_DEBOUNCE = 500; // ms idle after a move before persisting position
const COLLAPSE_FADE = 280; // ms shrink/fade before the backend swaps the windows

export function useAutoHide() {
  const [hidden, setHidden] = useState(false);
  const state = useRef({
    hidden: false,
    paused: false,
    collapsing: false,
    timer: null as ReturnType<typeof setTimeout> | null,
    saveTimer: null as ReturnType<typeof setTimeout> | null,
  });

  // Pet → ball: flag hidden so the window plays its shrink/fade, then let the
  // backend swap the pet window for the ball. Expansion is owned by the ball
  // (click → expand_from_ball → main-shown), which resets the flags below.
  const collapseToBall = async () => {
    const s = state.current;
    if (s.hidden || s.paused || s.collapsing) return;

    const win = getCurrentWindow();
    // While the panel is open the pet window is already hidden (not just
    // blurred) and the blur timer still fires — don't swap an invisible window.
    if (!(await win.isVisible())) return;

    s.collapsing = true;
    s.hidden = true;
    setHidden(true);
    await new Promise((r) => setTimeout(r, COLLAPSE_FADE));
    try {
      await invoke("collapse_to_ball");
    } catch (e) {
      console.error("Failed to collapse to ball:", e);
      s.hidden = false;
      setHidden(false);
    }
    s.collapsing = false;
  };

  const cancelTimer = () => {
    const s = state.current;
    if (s.timer) {
      clearTimeout(s.timer);
      s.timer = null;
    }
  };

  const startBlurTimer = () => {
    const s = state.current;
    if (s.paused || s.hidden) return;
    cancelTimer();
    s.timer = setTimeout(() => {
      if (!s.hidden && !s.paused) {
        collapseToBall();
      }
    }, BLUR_TIMEOUT);
  };

  // The window itself no longer expands on hover: while collapsed it's hidden
  // and the ball owns the click-to-expand. Enter/leave only manages the timer.
  const handleMouseEnter = () => {
    cancelTimer();
  };

  const pauseTimer = () => {
    const s = state.current;
    s.paused = true;
    cancelTimer();
  };

  const resumeTimer = () => {
    const s = state.current;
    s.paused = false;
  };

  useEffect(() => {
    const win = getCurrentWindow();
    let unlistenFocus: (() => void) | null = null;
    let unlistenMoved: (() => void) | null = null;
    let unlistenShown: (() => void) | null = null;
    // `onFocusChanged`/`onMoved`/`listen` register asynchronously. Under
    // StrictMode (mount → unmount → remount) and Vite HMR the cleanup can run
    // before the `await` resolves, leaving `unlisten*` null so the listener
    // leaks — and a leaked focus listener closes over the DISCARDED instance's
    // `state` ref, whose `paused` is always false, so it keeps auto-hiding the
    // window even after the live instance is pinned. The `cancelled` flag tears
    // down any listener that resolves after cleanup. (Same async-leak hazard
    // CLAUDE.md flags for the `turn` event, but each of these is a single owned
    // listener that must be unregistered, so the flag is the correct fix.)
    let cancelled = false;

    const setup = async () => {
      const focus = await win.onFocusChanged(({ payload: focused }) => {
        const s = state.current;
        if (s.paused) return;
        if (focused) {
          cancelTimer();
        } else if (!s.hidden) {
          startBlurTimer();
        }
      });
      if (cancelled) {
        focus();
        return;
      }
      unlistenFocus = focus;

      // Persist the user's window position so it reopens where they left it.
      // Skipped while hidden: the collapse never moves the window, and a save
      // fired mid-fade could race the real position.
      const moved = await win.onMoved(({ payload }) => {
        const s = state.current;
        if (s.hidden || s.paused) return;
        if (s.saveTimer) clearTimeout(s.saveTimer);
        s.saveTimer = setTimeout(() => {
          invoke("save_window_position", { x: payload.x, y: payload.y }).catch(
            (e) => console.error("Failed to save window position:", e),
          );
        }, SAVE_DEBOUNCE);
      });
      if (cancelled) {
        moved();
        return;
      }
      unlistenMoved = moved;

      // Expanded again from the ball (or the panel closed): the window just
      // reappeared, so reset the hide bookkeeping — otherwise the blur timer
      // would refuse to start and the pet could never auto-hide again.
      const shown = await win.listen<unknown>("main-shown", () => {
        const s = state.current;
        s.hidden = false;
        s.collapsing = false;
        setHidden(false);
        cancelTimer();
      });
      if (cancelled) {
        shown();
        return;
      }
      unlistenShown = shown;
    };

    setup();

    return () => {
      cancelled = true;
      cancelTimer();
      if (state.current.saveTimer) clearTimeout(state.current.saveTimer);
      unlistenFocus?.();
      unlistenMoved?.();
      unlistenShown?.();
    };
  }, []);

  return { hidden, handleMouseEnter, pauseTimer, resumeTimer, collapseToBall };
}
