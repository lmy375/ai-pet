import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { PanelSettings } from "./components/panel/PanelSettings";
import { PanelMemory } from "./components/panel/PanelMemory";
import { PanelChat } from "./components/panel/PanelChat";
import { Segmented } from "./components/ui/Segmented";
import { ExternalLinkIcon } from "./components/Icons";

const TABS = ["设置", "记忆", "聊天"] as const;
type Tab = (typeof TABS)[number];

export function PanelApp() {
  const [activeTab, setActiveTab] = useState<Tab>("设置");

  const openDebugWindow = () => {
    invoke("open_debug").catch(console.error);
  };

  return (
    <div className="flex h-screen w-full flex-col bg-slate-100">
      {/* Top nav */}
      <div className="flex shrink-0 items-center justify-between border-b border-slate-200/70 bg-white/80 px-4 py-2.5 backdrop-blur">
        <Segmented value={activeTab} options={TABS} onChange={setActiveTab} />
        <button
          onClick={openDebugWindow}
          title="在新窗口中打开调试日志"
          className="flex h-8 w-8 items-center justify-center rounded-lg text-slate-500 transition-colors hover:bg-slate-100 hover:text-slate-700"
        >
          <ExternalLinkIcon className="h-[18px] w-[18px]" />
        </button>
      </div>

      {/* Tab content */}
      <div className="flex-1 overflow-hidden">
        {activeTab === "设置" && <PanelSettings />}
        {activeTab === "记忆" && <PanelMemory />}
        {activeTab === "聊天" && <PanelChat />}
      </div>
    </div>
  );
}
