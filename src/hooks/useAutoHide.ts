import { useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";
import { useEvent } from "../lib/events";
import { useWindowPositionSave } from "./useWindowPositionSave";

const BLUR_TIMEOUT = 3000; // 3s after blur → collapse into the ball
const COLLAPSE_FADE = 280; // ms shrink/fade before the backend swaps the windows

export function useAutoHide() {
  const [hidden, setHidden] = useState(false);
  const [paused, setPaused] = useState(false);
  const state = useRef({
    hidden: false,
    paused: false,
    collapsing: false,
    timer: null as ReturnType<typeof setTimeout> | null,
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
    setPaused(true);
    cancelTimer();
  };

  const resumeTimer = () => {
    const s = state.current;
    s.paused = false;
    setPaused(false);
  };

  // Persist the user's window position so it reopens where they left it. Skipped
  // while hidden (the collapse never moves the window, and a save fired mid-fade
  // could race the real position) or pinned. Clamping always runs.
  useWindowPositionSave("save_window_position", !hidden && !paused);

  useEvent("tauri://focus", () => {
    if (state.current.paused) return;
    cancelTimer();
  });

  useEvent("tauri://blur", () => {
    const s = state.current;
    if (s.paused || s.hidden) return;
    startBlurTimer();
  });

  // Expanded again from the ball (or the panel closed): the window just
  // reappeared, so reset the hide bookkeeping — otherwise the blur timer would
  // refuse to start and the pet could never auto-hide again.
  useEvent("main-shown", () => {
    const s = state.current;
    s.hidden = false;
    s.collapsing = false;
    setHidden(false);
    cancelTimer();
  });

  useEffect(() => () => cancelTimer(), []);

  return { hidden, handleMouseEnter, pauseTimer, resumeTimer, collapseToBall };
}