import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  applyTheme,
  getStoredAccent,
  setStoredAccent,
  type Accent,
} from "../theme";

/// 跨窗口 accent 同步监听（receiver 端）。
///
/// 当 PanelApp 用户在 accent picker 切换主品牌色时会 `emit("accent-change")`。
/// 所有"用同套 CSS 变量做样式"的窗口都需要 listen 并 applyTheme，否则跨窗
/// 口体验不一致。
///
/// 051-part1：theme（light/dark）整体删除后本 hook 仅同步 accent。
///
/// 容错：accent payload 不在合法 enum 里时退到 "default"。
export function useThemeChangeSync(): void {
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    (async () => {
      unlisten = await listen<string>("accent-change", (event) => {
        const valid: Accent[] = ["default", "green", "purple", "orange", "rose"];
        const raw = event.payload as Accent;
        const next = valid.includes(raw) ? raw : "default";
        if (getStoredAccent() === next) return;
        setStoredAccent(next);
        applyTheme(next);
      });
    })();
    return () => {
      if (unlisten) unlisten();
    };
  }, []);
}
