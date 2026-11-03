import { useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useEvent } from "../lib/events";

const SAVE_DEBOUNCE = 500; // ms idle after a move before persisting position

type SavePositionCmd = "save_window_position" | "save_ball_position";

/**
 * Keep the window on screen while it is moved/resized and remember where the
 * user left it. Shared by the pet window (saves `window`) and the floating ball
 * (saves `ball`); the only difference is the command name.
 *
 * - Move/resize always clamp the window back into a display.
 * - A move schedules a debounced save; `enabled` gates the save (the pet passes
 *   false while hidden or pinned, the ball always saves).
 */
export function useWindowPositionSave(cmd: SavePositionCmd, enabled: boolean): void {
  const enabledRef = useRef(enabled);
  enabledRef.current = enabled;
  const saveTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const keepVisible = () => {
    invoke("keep_window_visible").catch((e) =>
      console.error("Failed to keep window on screen:", e),
    );
  };

  const scheduleSave = () => {
    if (saveTimer.current) clearTimeout(saveTimer.current);
    saveTimer.current = setTimeout(() => {
      saveTimer.current = null;
      invoke(cmd).catch((e) => console.error(`Failed to run "${cmd}":`, e));
    }, SAVE_DEBOUNCE);
  };

  useEvent("tauri://move", () => {
    keepVisible();
    if (enabledRef.current) scheduleSave();
  });
  useEvent("tauri://resize", keepVisible);

  useEffect(
    () => () => {
      if (saveTimer.current) clearTimeout(saveTimer.current);
    },
    [],
  );
}