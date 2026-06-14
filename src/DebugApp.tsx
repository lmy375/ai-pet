import { useState } from "react";
import { PanelDebug } from "./components/panel/PanelDebug";
import { LlmLogView } from "./components/panel/LlmLogView";
import { Segmented } from "./components/ui/Segmented";

const TABS = ["应用日志", "LLM 日志"] as const;
type Tab = (typeof TABS)[number];

export function DebugApp() {
  const [activeTab, setActiveTab] = useState<Tab>("应用日志");

  return (
    <div className="flex h-screen w-full flex-col bg-slate-100">
      {/* Top nav */}
      <div className="flex shrink-0 items-center border-b border-slate-200/70 bg-white/80 px-4 py-2.5 backdrop-blur">
        <Segmented value={activeTab} options={TABS} onChange={setActiveTab} />
      </div>

      {/* Tab content */}
      <div className="flex-1 overflow-hidden">
        {activeTab === "应用日志" && <PanelDebug />}
        {activeTab === "LLM 日志" && <LlmLogView />}
      </div>
    </div>
  );
}
