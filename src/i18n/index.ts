import { useSyncExternalStore } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import zhJson from "./locales/zh.json";
import enJson from "./locales/en.json";

export type Lang = "zh" | "en";
export type TKey = keyof typeof zhJson;

// `zh.json` is the authoritative key set; typing `en` as Record<TKey, string>
// makes a missing English key a compile-time error (`npm run build` catches it).
const zh: Record<TKey, string> = zhJson;
const en: Record<TKey, string> = enJson;
const tables: Record<Lang, Record<TKey, string>> = { zh, en };

/** Look up `key` for `lang`, fill `{var}` placeholders, fall back zh → raw key. */
export function translate(lang: Lang, key: TKey, vars?: Record<string, string | number>): string {
  const s = tables[lang]?.[key] ?? zh[key] ?? String(key);
  if (!vars) return s;
  return s.replace(/\{(\w+)\}/g, (m, name) => (name in vars ? String(vars[name]) : m));
}

// Single shared language store: every component reads the same value through one
// `get_settings` call and one `settings-changed` listener (instead of each
// component subscribing on its own — many components call useI18n()).
let currentLang: Lang = "zh";
const listeners = new Set<() => void>();
let started = false;

async function loadLang() {
  try {
    const s = await invoke<{ language?: string }>("get_settings");
    const next: Lang = s.language === "en" ? "en" : "zh";
    if (next !== currentLang) {
      currentLang = next;
      listeners.forEach((l) => l());
    }
  } catch {
    // Keep the current language if settings can't be read.
  }
}

function subscribe(cb: () => void) {
  if (!started) {
    started = true;
    loadLang();
    listen("settings-changed", loadLang);
  }
  listeners.add(cb);
  return () => listeners.delete(cb);
}

const getSnapshot = () => currentLang;

export function useI18n() {
  const lang = useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
  return {
    lang,
    t: (key: TKey, vars?: Record<string, string | number>) => translate(lang, key, vars),
  };
}
