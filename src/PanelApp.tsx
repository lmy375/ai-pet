import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { PanelSettings } from "./components/panel/PanelSettings";
import { PanelChat } from "./components/panel/PanelChat";
import { PanelGroup } from "./components/panel/PanelGroup";
import { PanelTasks } from "./components/panel/PanelTasks";
import { Segmented } from "./components/ui/Segmented";
import { BugIcon } from "./components/Icons";
import { useI18n } from "./i18n";

type Tab = "chat" | "group" | "tasks" | "settings";

export function PanelApp() {
  const { t } = useI18n();
  const [activeTab, setActiveTab] = useState<Tab>("chat");

  const tabs = [
    { value: "chat" as const, label: t("panel.tab.chat") },
    { value: "group" as const, label: t("panel.tab.group") },
    { value: "tasks" as const, label: t("panel.tab.tasks") },
    { value: "settings" as const, label: t("panel.tab.settings") },
  ];

  const openDebugWindow = () => {
    invoke("open_debug").catch(console.error);
  };

  return (
    <div className="flex h-screen w-full flex-col bg-slate-100">
      {/* Top nav */}
      <div className="flex shrink-0 items-center justify-between border-b border-slate-200/70 bg-white/80 px-4 py-2.5 backdrop-blur">
        <Segmented value={activeTab} options={tabs} onChange={setActiveTab} />
        <button
          onClick={openDebugWindow}
          title={t("panel.openDebug")}
          className="flex h-8 w-8 items-center justify-center rounded-lg text-slate-500 transition-colors hover:bg-slate-100 hover:text-slate-700"
        >
          <BugIcon className="h-[18px] w-[18px]" />
        </button>
      </div>

      {/* Tab content */}
      <div className="flex-1 overflow-hidden">
        {activeTab === "settings" && <PanelSettings />}
        {activeTab === "chat" && <PanelChat />}
        {activeTab === "group" && <PanelGroup />}
        {activeTab === "tasks" && <PanelTasks />}
      </div>
    </div>
  );
}
