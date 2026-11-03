import type { EventCallback } from "@tauri-apps/api/event";
import { useEvent } from "../lib/events";

/**
 * Subscribe to a Tauri event for the component's lifetime, passing the raw Tauri
 * event object to `handler` (kept for the existing call sites).
 *
 * Delegates to `useEvent`, so the listener is registered once per event name and
 * the handler is always the latest render's — it no longer has to be
 * self-contained.
 */
export function useTauriEvent<T>(name: string, handler: EventCallback<T>) {
  useEvent<T>(name, (payload) => handler({ event: name, id: 0, payload }));
}