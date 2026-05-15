import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  applyTheme,
  getStoredAccent,
  getStoredTheme,
  setStoredAccent,
  setStoredTheme,
  type Accent,
} from "../theme";

/// 跨窗口主题 / 强调色同步监听（receiver 端）。
///
/// 当 PanelApp 用户在右上角点 ☀️/🌙 toggle 主题，或切换 accent 色时，会
/// `emit("theme-change", ...)` / `emit("accent-change", ...)`。所有"用同套
/// CSS 变量做样式"的窗口都需要 listen 并 applyTheme，否则跨窗口体验不一致。
///
/// 此 hook 仅 receiver 端（App.tsx pet 窗 / DebugApp.tsx 调试窗）：listen +
/// dedup + applyTheme。PanelApp 自己持 setTheme React state，逻辑不同，不用
/// 此 hook（它在 setTheme 时已立即 applyTheme，再 listen 自身 emit 会无效）。
///
/// 容错：accent payload 不在合法 enum 里时退到 "default"，与既有逻辑一致。
export function useThemeChangeSync(): void {
  useEffect(() => {
    let unlistenTheme: (() => void) | undefined;
    let unlistenAccent: (() => void) | undefined;
    (async () => {
      unlistenTheme = await listen<string>("theme-change", (event) => {
        const next = event.payload === "dark" ? "dark" : "light";
        if (getStoredTheme() === next) return;
        setStoredTheme(next);
        applyTheme(next, getStoredAccent());
      });
      unlistenAccent = await listen<string>("accent-change", (event) => {
        const valid: Accent[] = ["default", "green", "purple", "orange", "rose"];
        const raw = event.payload as Accent;
        const next = valid.includes(raw) ? raw : "default";
        if (getStoredAccent() === next) return;
        setStoredAccent(next);
        applyTheme(getStoredTheme(), next);
      });
    })();
    return () => {
      if (unlistenTheme) unlistenTheme();
      if (unlistenAccent) unlistenAccent();
    };
  }, []);
}
