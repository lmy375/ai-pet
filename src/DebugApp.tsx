import { useState } from "react";
import { PanelDebug } from "./components/panel/PanelDebug";
import { LlmLogView } from "./components/panel/LlmLogView";
import { Segmented } from "./components/ui/Segmented";
import { useI18n } from "./i18n";

type Tab = "app" | "llm";

export function DebugApp() {
  const { t } = useI18n();
  const [activeTab, setActiveTab] = useState<Tab>("app");

  const tabs = [
    { value: "app" as const, label: t("debug.tab.app") },
    { value: "llm" as const, label: t("debug.tab.llm") },
  ];

  return (
    <div className="flex h-screen w-full flex-col bg-slate-100">
      {/* Top nav */}
      <div className="flex shrink-0 items-center border-b border-slate-200/70 bg-white/80 px-4 py-2.5 backdrop-blur">
        <Segmented value={activeTab} options={tabs} onChange={setActiveTab} />
      </div>

      {/* Tab content */}
      <div className="flex-1 overflow-hidden">
        {activeTab === "app" && <PanelDebug />}
        {activeTab === "llm" && <LlmLogView />}
      </div>
    </div>
  );
}
