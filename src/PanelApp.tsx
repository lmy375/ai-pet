import { useState } from "react";
import { PanelSettings } from "./components/panel/PanelSettings";
import { PanelChat } from "./components/panel/PanelChat";
import { PanelDebug } from "./components/panel/PanelDebug";

const TABS = ["设置", "聊天", "调试"] as const;
type Tab = (typeof TABS)[number];

export function PanelApp() {
  const [activeTab, setActiveTab] = useState<Tab>("设置");

  return (
    <div style={{ width: "100%", height: "100vh", display: "flex", flexDirection: "column", background: "#f8fafc" }}>
      {/* Tab bar */}
      <div style={{ display: "flex", borderBottom: "1px solid #e2e8f0", background: "#fff", flexShrink: 0 }}>
        {TABS.map((tab) => (
          <button
            key={tab}
            onClick={() => setActiveTab(tab)}
            style={{
              flex: 1,
              padding: "12px 0",
              border: "none",
              borderBottom: activeTab === tab ? "2px solid #0ea5e9" : "2px solid transparent",
              background: "transparent",
              color: activeTab === tab ? "#0ea5e9" : "#64748b",
              fontWeight: activeTab === tab ? 600 : 400,
              fontSize: "14px",
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
        {activeTab === "设置" && <PanelSettings />}
        {activeTab === "聊天" && <PanelChat />}
        {activeTab === "调试" && <PanelDebug />}
      </div>
    </div>
  );
}
