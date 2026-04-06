import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { PanelSettings } from "./components/panel/PanelSettings";
import { PanelChat } from "./components/panel/PanelChat";
import { PanelMemory } from "./components/panel/PanelMemory";

const TABS = ["设置", "聊天", "记忆"] as const;
type Tab = (typeof TABS)[number];

export function PanelApp() {
  const [activeTab, setActiveTab] = useState<Tab>("设置");

  const openDebugWindow = () => {
    invoke("open_debug").catch(console.error);
  };

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
        <button
          onClick={openDebugWindow}
          style={{
            padding: "12px 16px",
            border: "none",
            borderBottom: "2px solid transparent",
            background: "transparent",
            color: "#64748b",
            fontWeight: 400,
            fontSize: "14px",
            cursor: "pointer",
            transition: "all 0.2s",
          }}
          title="在新窗口中打开调试日志"
        >
          调试 ↗
        </button>
      </div>

      {/* Tab content */}
      <div style={{ flex: 1, overflow: "hidden" }}>
        {activeTab === "设置" && <PanelSettings />}
        {activeTab === "聊天" && <PanelChat />}
        {activeTab === "记忆" && <PanelMemory />}
      </div>
    </div>
  );
}
