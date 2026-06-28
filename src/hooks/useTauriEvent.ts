import { useEffect } from "react";
import { listen, type EventCallback } from "@tauri-apps/api/event";

/**
 * Subscribe to a Tauri event for the component's lifetime, handling the async
 * `listen()` → store-unlisten → cleanup dance so callers only write the handler.
 *
 * The handler is captured once on mount (the listener is registered with no
 * deps), so it must be self-contained — read mutable values through refs, not
 * closure variables. This deliberately mirrors the always-registered listeners
 * it replaces: it does NOT use a `cancelled`/self-cancel flag (that pattern
 * previously caused a zero-notification regression for `background-finished` —
 * see CLAUDE.md). Idempotency is the handler's job, e.g. dedup by task id.
 */
export function useTauriEvent<T>(name: string, handler: EventCallback<T>) {
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    listen<T>(name, handler).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
}
