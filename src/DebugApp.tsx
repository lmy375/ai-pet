import { useState } from "react";
import { PanelDebug } from "./components/panel/PanelDebug";
import { LlmLogView } from "./components/panel/LlmLogView";

const TABS = ["应用日志", "LLM 日志"] as const;
type Tab = (typeof TABS)[number];

export function DebugApp() {
  const [activeTab, setActiveTab] = useState<Tab>("应用日志");

  return (
    <div style={{ width: "100%", height: "100vh", display: "flex", flexDirection: "column", background: "#f8fafc" }}>
      {/* Tab bar */}
      <div style={{ display: "flex", borderBottom: "1px solid #e2e8f0", background: "#fff" }}>
        {TABS.map((tab) => (
          <button
            key={tab}
            onClick={() => setActiveTab(tab)}
            style={{
              padding: "10px 20px",
              border: "none",
              borderBottom: activeTab === tab ? "2px solid #0ea5e9" : "2px solid transparent",
              background: "transparent",
              color: activeTab === tab ? "#0ea5e9" : "#64748b",
              fontWeight: activeTab === tab ? 600 : 400,
              fontSize: "13px",
              cursor: "pointer",
              transition: "all 0.2s",
            }}
          >
            {tab}
          </button>
        ))}
      </div>

      {/* Tab content */}
      <div style={{ flex: 1, overflow: "hidden" }}>
        {activeTab === "应用日志" && <PanelDebug />}
        {activeTab === "LLM 日志" && <LlmLogView />}
      </div>
    </div>
  );
}
