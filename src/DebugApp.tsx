import { useCallback, useEffect, useState } from "react";
import { PanelDebug } from "./components/panel/PanelDebug";
import { LlmLogView } from "./components/panel/LlmLogView";
import { PanelDebugStats } from "./components/panel/PanelDebugStats";
import { PanelDebugLogs } from "./components/panel/PanelDebugLogs";
import { useTabKeyboardShortcut } from "./hooks/useTabKeyboardShortcut";
import { useThemeChangeSync } from "./hooks/useThemeChangeSync";
import { applyTheme, getStoredAccent } from "./theme";

const TABS = ["应用", "日志", "LLM 日志", "统计"] as const;
type Tab = (typeof TABS)[number];

// 启动时立刻把 accent 刷到 documentElement —— 与 PanelApp / App 入口一致，
// 让 CSS var（shadow / tint / accent）立刻可用。
applyTheme(getStoredAccent());

export function DebugApp() {
  // sessionStorage hop：PanelDebug "🔄 reload" 在 reload 前写当前 tab，
  // reload 后读回让用户停在原 tab 而非"应用"默认。读完即清。
  const [activeTab, setActiveTab] = useState<Tab>(() => {
    try {
      const raw = sessionStorage.getItem("pet-debug-reload-tab");
      if (raw) {
        sessionStorage.removeItem("pet-debug-reload-tab");
        if (raw === "应用" || raw === "日志" || raw === "LLM 日志" || raw === "统计") {
          return raw as Tab;
        }
      }
    } catch {
      // 退默认
    }
    return "应用";
  });

  // 跨窗口主题 / 强调色同步走共享 hook（与 App.tsx 桌面宠物窗口同一份）。
  useThemeChangeSync();

  /// iter #395: pet-debug-deeplink 消费 — caller（如 ChatMini ambient
  /// hint chip click）写 localStorage `pet-debug-deeplink` =
  /// `{ tab, scrollAnchor?, ts }` + invoke("open_debug")；DebugApp
  /// 在 mount + storage 事件两路径消费（已开 / 未开两 case）。TTL
  /// 10s 防过期 deeplink 在用户后续手动开 debug 时误触发。scroll-
  /// Anchor → 找 id=`pet-debug-anchor-<value>` 元素 + scrollIntoView。
  ///
  /// 同既有 pet-panel-deeplink 模板（PanelApp consumePanelDeeplink），
  /// 但 key 独立避免与 panel deeplink 冲突 + tab 名空间不同。
  const consumeDebugDeeplink = useCallback(() => {
    let raw: string | null = null;
    try {
      raw = localStorage.getItem("pet-debug-deeplink");
    } catch {
      return;
    }
    if (!raw) return;
    try {
      localStorage.removeItem("pet-debug-deeplink");
    } catch {
      // ignore — TTL 兜底
    }
    let parsed: unknown;
    try {
      parsed = JSON.parse(raw);
    } catch {
      return;
    }
    if (!parsed || typeof parsed !== "object") return;
    const p = parsed as {
      tab?: unknown;
      scrollAnchor?: unknown;
      ts?: unknown;
    };
    if (typeof p.ts !== "number" || Date.now() - p.ts > 10_000) return;
    if (typeof p.tab === "string" && (TABS as readonly string[]).includes(p.tab)) {
      setActiveTab(p.tab as Tab);
    }
    if (typeof p.scrollAnchor === "string" && p.scrollAnchor.trim()) {
      const anchor = p.scrollAnchor.trim();
      // 等下一帧让 setActiveTab 生效 + PanelDebug 完成 mount + 渲染
      // 后再 scroll；50ms 余量给 children 完成 first paint（避免 anchor
      // 元素还未在 DOM 时 getElementById null）。
      window.setTimeout(() => {
        const el = document.getElementById(`pet-debug-anchor-${anchor}`);
        if (el) {
          el.scrollIntoView({ behavior: "smooth", block: "start" });
          // 短暂 flash 高亮让 owner 看到目标位置 — 与既有 mem-flash
          // / chat-match 高亮风格一致。
          el.style.transition = "background 600ms ease-out";
          const prev = el.style.background;
          el.style.background =
            "color-mix(in srgb, var(--pet-color-accent) 24%, transparent)";
          window.setTimeout(() => {
            el.style.background = prev;
          }, 1200);
        }
      }, 50);
    }
  }, []);
  // 已开 debug 窗：pet 窗 setItem 触发 storage 事件 → 立即消费
  useEffect(() => {
    const onStorage = (e: StorageEvent) => {
      if (e.key !== "pet-debug-deeplink") return;
      if (!e.newValue) return;
      consumeDebugDeeplink();
    };
    window.addEventListener("storage", onStorage);
    return () => window.removeEventListener("storage", onStorage);
  }, [consumeDebugDeeplink]);
  // 未开 debug 窗：open_debug 后 DebugApp 首次 mount → 这里读
  useEffect(() => {
    consumeDebugDeeplink();
  }, [consumeDebugDeeplink]);

  // ⌘1 – ⌘4（含 Ctrl 等价）跳到 N 号 tab —— 共用 useTabKeyboardShortcut hook
  // 与 PanelApp 同模式（hook 内部按 tabs.length 自动适配键位范围）。
  useTabKeyboardShortcut(TABS, setActiveTab);

  return (
    <div
      style={{
        width: "100%",
        height: "100vh",
        display: "flex",
        flexDirection: "column",
        background: "var(--pet-color-bg)",
      }}
    >
      {/* 共享 base 抛光（与 PanelApp.tsx 节奏对齐）：tab pill 指示器 +
          inactive hover 预告短条。tab bar 不走 inline borderBottom 而是
          CSS ::after，让 active / hover 形态统一。 */}
      <style>{`
        .pet-debug-tab {
          transition: color 140ms ease-out, background-color 140ms ease-out;
        }
        .pet-debug-tab:hover:not([data-active="true"]) {
          background: color-mix(in srgb, var(--pet-color-accent) 8%, transparent);
          color: var(--pet-color-fg);
        }
        .pet-debug-tab[data-active="true"]::after {
          content: "";
          position: absolute;
          left: 50%;
          bottom: -1px;
          transform: translateX(-50%);
          width: 28px;
          height: 3px;
          border-radius: 2px;
          background: var(--pet-color-accent);
          box-shadow: 0 0 8px color-mix(in srgb, var(--pet-color-accent) 50%, transparent);
        }
        .pet-debug-tab:hover:not([data-active="true"])::after {
          content: "";
          position: absolute;
          left: 50%;
          bottom: -1px;
          transform: translateX(-50%);
          width: 16px;
          height: 3px;
          border-radius: 2px;
          background: color-mix(in srgb, var(--pet-color-accent) 50%, transparent);
        }
      `}</style>
      {/* Tab bar */}
      <div
        style={{
          display: "flex",
          borderBottom: "1px solid var(--pet-color-border)",
          background: "var(--pet-color-card)",
        }}
      >
        {TABS.map((tab) => {
          const isActive = activeTab === tab;
          return (
            <button
              key={tab}
              className="pet-debug-tab"
              data-active={isActive}
              onClick={() => setActiveTab(tab)}
              style={{
                padding: "10px 20px",
                border: "none",
                // 永远 2px transparent 占位；视觉指示由 CSS ::after。
                borderBottom: "2px solid transparent",
                background: "transparent",
                color: isActive ? "var(--pet-color-accent)" : "var(--pet-color-muted)",
                fontWeight: isActive ? 600 : 500,
                fontSize: "13px",
                letterSpacing: 0.2,
                cursor: isActive ? "default" : "pointer",
                position: "relative",
              }}
            >
              {tab}
            </button>
          );
        })}
      </div>

      {/* Tab content */}
      <div style={{ flex: 1, overflow: "hidden" }}>
        {activeTab === "应用" && <PanelDebug />}
        {activeTab === "日志" && <PanelDebugLogs />}
        {activeTab === "LLM 日志" && <LlmLogView />}
        {activeTab === "统计" && <PanelDebugStats />}
      </div>
    </div>
  );
}
