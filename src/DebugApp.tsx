import { useState } from "react";
import { PanelDebug } from "./components/panel/PanelDebug";
import { LlmLogView } from "./components/panel/LlmLogView";
import { PanelDebugStats } from "./components/panel/PanelDebugStats";
import { PanelDebugLogs } from "./components/panel/PanelDebugLogs";
import { useTabKeyboardShortcut } from "./hooks/useTabKeyboardShortcut";
import { useThemeChangeSync } from "./hooks/useThemeChangeSync";
import { applyTheme, getStoredAccent, getStoredTheme } from "./theme";

const TABS = ["应用", "日志", "LLM 日志", "统计"] as const;
type Tab = (typeof TABS)[number];

// 启动时立刻把存的主题刷到 documentElement —— 与 PanelApp / App 入口一致，
// 避免 light flash + 让 CSS var（shadow / tint / accent）立刻可用。
applyTheme(getStoredTheme(), getStoredAccent());

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
