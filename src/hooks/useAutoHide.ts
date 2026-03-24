import { useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { currentMonitor } from "@tauri-apps/api/window";
import { PhysicalPosition } from "@tauri-apps/api/dpi";

const BLUR_TIMEOUT = 3000; // 3s after blur → slide to edge
const TAB_WIDTH = 12; // pixels visible when hidden (the "tab")
const ANIM_DURATION = 300; // ms for slide animation
const ANIM_STEPS = 20;

export function useAutoHide() {
  const [hidden, setHidden] = useState(false);
  const state = useRef({
    hidden: false,
    paused: false,
    animating: false,
    savedX: null as number | null,
    timer: null as ReturnType<typeof setTimeout> | null,
  });

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
    const startX = pos.x;
    const y = pos.y;

    for (let i = 1; i <= ANIM_STEPS; i++) {
      const progress = i / ANIM_STEPS;
      // ease-out cubic
      const eased = 1 - Math.pow(1 - progress, 3);
      const x = Math.round(startX + (targetX - startX) * eased);
      await win.setPosition(new PhysicalPosition(x, y));
      await new Promise((r) => setTimeout(r, ANIM_DURATION / ANIM_STEPS));
    }

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
    const startX = pos.x;
    const y = pos.y;

    s.animating = true;

    for (let i = 1; i <= ANIM_STEPS; i++) {
      const progress = i / ANIM_STEPS;
      // ease-out cubic
      const eased = 1 - Math.pow(1 - progress, 3);
      const x = Math.round(startX + (targetX - startX) * eased);
      await win.setPosition(new PhysicalPosition(x, y));
      await new Promise((r) => setTimeout(r, ANIM_DURATION / ANIM_STEPS));
    }

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

    const setup = async () => {
      unlistenFocus = await win.onFocusChanged(({ payload: focused }) => {
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
    };

    setup();

    return () => {
      cancelTimer();
      unlistenFocus?.();
    };
  }, []);

  return { hidden, handleMouseEnter, pauseTimer, resumeTimer };
}
