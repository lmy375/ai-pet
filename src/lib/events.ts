import { useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

/** Payload-first event callback: subscribers never see the raw Tauri event. */
export type EventHandler<T> = (payload: T) => void;

// One subscriber set per event name, one `listen()` per name for the lifetime
// of the webview. Listeners are never unregistered: the event names are fixed
// and few, and keeping them alive removes the async-registration race that
// StrictMode/HMR remounts used to guard against with `cancelled` flags. Any
// number of components may subscribe to the same event.
//
// App events keep the default `Any` target (each window receives `app.emit`).
// Window events (`tauri://move`, `tauri://focus`, …) are scoped to the current
// window label so the pet, panel and ball webviews don't see each other's.
const subscribers = new Map<string, Set<EventHandler<unknown>>>();

function windowScopedTarget(name: string) {
  return name.startsWith("tauri://")
    ? { target: { kind: "Window" as const, label: getCurrentWindow().label } }
    : undefined;
}

/** Add a handler for a Tauri event; returns an unsubscribe function. */
export function subscribe<T>(name: string, handler: EventHandler<T>): () => void {
  let handlers = subscribers.get(name);
  if (!handlers) {
    handlers = new Set();
    subscribers.set(name, handlers);
    const dispatch = handlers;
    listen<T>(
      name,
      (event) => {
        // Copy so a handler may unsubscribe while the event is dispatched.
        for (const fn of [...dispatch]) fn(event.payload);
      },
      windowScopedTarget(name),
    ).catch((e) => console.error(`Failed to listen for "${name}":`, e));
  }

  const set = handlers;
  const fn = handler as EventHandler<unknown>;
  set.add(fn);
  return () => {
    set.delete(fn);
  };
}

/**
 * Subscribe for the component's lifetime. `handler` is kept in a ref so it
 * always sees the latest props/state without re-subscribing; unmounting only
 * drops the subscriber (the underlying listener stays registered), and events
 * arriving after unmount are ignored.
 */
export function useEvent<T>(name: string, handler: EventHandler<T>): void {
  const handlerRef = useRef(handler);
  handlerRef.current = handler;

  useEffect(() => {
    let mounted = true;
    const unsubscribe = subscribe<T>(name, (payload) => {
      if (mounted) handlerRef.current(payload);
    });
    return () => {
      mounted = false;
      unsubscribe();
    };
  }, [name]);
}