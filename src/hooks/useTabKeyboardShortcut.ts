import { useEffect } from "react";

/// 给"窗口顶部 N 个 tab"挂 `⌘1` – `⌘N` 跳转快捷键（Ctrl 等价）。
///
/// PanelApp 与 DebugApp 都用这套肌肉记忆，逻辑完全一致：
/// - 修饰键 `metaKey || ctrlKey`，且 `!shiftKey && !altKey`
/// - 数字 `1` – `9` 映射 `tabs[idx-1]`；超出 tabs.length → 忽略
/// - INPUT / TEXTAREA / contenteditable 聚焦时让出键位（用户在输入框里继续打字）
/// - preventDefault 防 webview 默认行为
///
/// `tabs` 是 readonly 类型数组（含 `as const` 推导）；T 自动绑到 tab label
/// union。`setActiveTab` 接受 `T` 参数 —— 与既有 `useState<Tab>` setter 兼容。
export function useTabKeyboardShortcut<T extends string>(
  tabs: ReadonlyArray<T>,
  setActiveTab: (t: T) => void,
): void {
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (!(e.metaKey || e.ctrlKey)) return;
      if (e.shiftKey || e.altKey) return;
      // "1" - "9"。数字超出 tabs.length → 忽略，不抢键位。
      const idx = "123456789".indexOf(e.key);
      if (idx < 0 || idx >= tabs.length) return;
      const t = e.target as HTMLElement | null;
      if (t) {
        const tag = t.tagName;
        if (tag === "INPUT" || tag === "TEXTAREA" || t.isContentEditable) return;
      }
      e.preventDefault();
      setActiveTab(tabs[idx]);
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [tabs, setActiveTab]);
}
