import { useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { currentMonitor } from "@tauri-apps/api/window";
import { PhysicalPosition } from "@tauri-apps/api/dpi";
import { invoke } from "@tauri-apps/api/core";

const BLUR_TIMEOUT = 3000; // 3s after blur → slide to edge
const TAB_WIDTH = 12; // pixels visible when hidden (the "tab")
const ANIM_DURATION = 300; // ms for slide animation
const ANIM_STEPS = 20;
const SAVE_DEBOUNCE = 500; // ms idle after a move before persisting position

export function useAutoHide() {
  const [hidden, setHidden] = useState(false);
  const state = useRef({
    hidden: false,
    paused: false,
    animating: false,
    savedX: null as number | null,
    timer: null as ReturnType<typeof setTimeout> | null,
    saveTimer: null as ReturnType<typeof setTimeout> | null,
  });

  // Eased (ease-out cubic) horizontal slide from startX to targetX over
  // ANIM_DURATION. Shared by slideToEdge/slideBack — only the target differs.
  const animateTo = async (
    win: ReturnType<typeof getCurrentWindow>,
    startX: number,
    targetX: number,
    y: number,
  ) => {
    for (let i = 1; i <= ANIM_STEPS; i++) {
      const progress = i / ANIM_STEPS;
      const eased = 1 - Math.pow(1 - progress, 3);
      const x = Math.round(startX + (targetX - startX) * eased);
      await win.setPosition(new PhysicalPosition(x, y));
      await new Promise((r) => setTimeout(r, ANIM_DURATION / ANIM_STEPS));
    }
  };

  const slideToEdge = async () => {
    const s = state.current;
    if (s.hidden || s.paused || s.animating) return;

    const win = getCurrentWindow();
    const monitor = await currentMonitor();
    if (!monitor) return;

    const pos = await win.outerPosition();

    s.savedX = pos.x;
    s.animating = true;

    // Target: right edge of screen, leaving TAB_WIDTH visible
    // TAB_WIDTH is in logical pixels, scale to physical
    const scale = window.devicePixelRatio || 1;
    const tabPhysical = Math.round(TAB_WIDTH * scale);
    const targetX = monitor.position.x + monitor.size.width - tabPhysical;

    await animateTo(win, pos.x, targetX, pos.y);

    s.hidden = true;
    s.animating = false;
    setHidden(true);
  };

  const slideBack = async () => {
    const s = state.current;
    if (!s.hidden || s.animating || s.savedX === null) return;

    const win = getCurrentWindow();
    const pos = await win.outerPosition();
    const targetX = s.savedX;

    s.animating = true;

    await animateTo(win, pos.x, targetX, pos.y);

    s.hidden = false;
    s.animating = false;
    s.savedX = null;
    setHidden(false);
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
        slideToEdge();
      }
    }, BLUR_TIMEOUT);
  };

  // Called when mouse enters the tab area (visible strip)
  const handleMouseEnter = () => {
    const s = state.current;
    if (s.hidden && !s.animating) {
      slideBack();
    }
    cancelTimer();
  };

  const pauseTimer = () => {
    const s = state.current;
    s.paused = true;
    cancelTimer();
    if (s.hidden) slideBack();
  };

  const resumeTimer = () => {
    const s = state.current;
    s.paused = false;
  };

  useEffect(() => {
    const win = getCurrentWindow();
    let unlistenFocus: (() => void) | null = null;
    let unlistenMoved: (() => void) | null = null;
    // `onFocusChanged`/`onMoved` register asynchronously. Under StrictMode
    // (mount → unmount → remount) and Vite HMR the cleanup can run before the
    // `await` resolves, leaving `unlisten*` null so the listener leaks — and a
    // leaked focus listener closes over the DISCARDED instance's `state` ref,
    // whose `paused` is always false, so it keeps auto-hiding the window even
    // after the live instance is pinned. The `cancelled` flag tears down any
    // listener that resolves after cleanup. (Same async-leak hazard CLAUDE.md
    // flags for `background-finished`, but here a single owned listener must be
    // unregistered, so the flag is the correct fix rather than dedup.)
    let cancelled = false;

    const setup = async () => {
      const focus = await win.onFocusChanged(({ payload: focused }) => {
        const s = state.current;
        if (s.paused) return;
        if (focused) {
          cancelTimer();
          if (s.hidden) slideBack();
        } else {
          if (!s.hidden) {
            startBlurTimer();
          }
        }
      });
      if (cancelled) {
        focus();
        return;
      }
      unlistenFocus = focus;

      // Persist the user's window position so it reopens where they left it.
      // Skip auto-hide / animation moves so the edge "tab" position is never
      // saved as the real position.
      const moved = await win.onMoved(({ payload }) => {
        const s = state.current;
        if (s.hidden || s.animating || s.paused) return;
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
    };

    setup();

    return () => {
      cancelled = true;
      cancelTimer();
      if (state.current.saveTimer) clearTimeout(state.current.saveTimer);
      unlistenFocus?.();
      unlistenMoved?.();
    };
  }, []);

  return { hidden, handleMouseEnter, pauseTimer, resumeTimer };
}
