# CLAUDE.md

## Windows
- Pet window label = `main` (tauri.conf.json), Panel Chat window label = `panel` (commands/window.rs).
- Both windows render `useChat` and share ONE conversation, but each holds its own in-memory copy
  (`items` + `messagesRef`); disk (`save_session`/`load_session`) is the only shared state. Two
  rules keep them in sync — don't remove either:
  - **Focus-reload**: on focus each window calls `set_active_window(label)` and reloads the active
    session (`useChat.ts` focus effect). **Reload-before-send**: `sendMessage` reloads the current
    session before appending. Together these stop one window from showing stale history or
    clobbering what the other just saved (`save_session` is last-writer-wins).
  - **Active-window routing**: `background-finished` is emitted to the active window only
    (`active_window_label` in window.rs; used by `TauriNotifier` and `kill_task`). Both windows
    listen, but exactly one receives each completion, so a shared session never gets two
    injections. Do NOT revert to hard-coding `emit_to("main")` or a `label === "main"` listener guard.
- The `main` window is configured `visible: false` and is shown by `restore_main_window()` in
  lib.rs `setup()` (after restoring its saved position) to avoid a center-flash. If you remove
  that call the pet window will never appear. Position is saved (debounced) from `useAutoHide`
  on move, skipping auto-hide/animation moves so the edge "tab" position is never persisted.
- `background-finished` handling must be IDEMPOTENT, not "exactly one listener". `listen()`
  resolves async, so under StrictMode (mount→unmount→remount) and Vite HMR the listener can
  leak (→ fires twice) OR a cancel-after-await dance can unregister the survivor (→ fires zero
  times). Both have happened. The fix in useChat.ts: keep the listener always registered and
  dedup completions by `taskId` (`seenTaskIdsRef`). Do NOT reintroduce a `cancelled`/self-cancel
  flag to enforce a single listener — that's what caused the zero-notification regression.

## Known issues / gotchas

### Live2D blank after the pet collapses (RECURRING — has regressed multiple times)
Symptom: after the pet auto-hides to the screen edge and slides back (or the window is
occluded/minimized), the Live2D model renders blank/frozen.
- The Live2D canvas MUST stay mounted across auto-hide. Never gate the `<Live2DCharacter>`
  `<div>` behind `!hidden` in `src/App.tsx` — unmounting tears down the PIXI/WebGL app and it
  does not re-init cleanly. Auto-hide (`src/hooks/useAutoHide.ts`) only moves the window via
  `setPosition`; it does not (and must not) unmount the canvas.
- Root cause: the WebGL context is lost while the window is offscreen/occluded and PIXI does
  not restore it on its own. CURRENT FIX (`Live2DCharacter.tsx`): it listens for
  `webglcontextlost` (calls `preventDefault` so restore can fire) and `webglcontextrestored`
  (fully rebuilds the PIXI app + reloads the model). Listeners are on the canvas, not the app,
  so they survive teardown. Keep this — removing it brings the blank-canvas bug back.
- Before changing anything around auto-hide, window show/hide, or the Live2D mount, verify the
  model still renders after a full collapse → expand cycle.
