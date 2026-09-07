import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { PanelSettings } from "./components/panel/PanelSettings";
import { PanelChat } from "./components/panel/PanelChat";
import { PanelGroup } from "./components/panel/PanelGroup";
import { PanelTasks } from "./components/panel/PanelTasks";
import { IconActionButton } from "./components/ui/IconButton";
import { BugIcon, PawIcon } from "./components/Icons";
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
    <div className="flex h-screen w-full flex-col bg-canvas text-ink">
      {/* Top bar: brand, section tabs, window-level actions */}
      <header className="flex shrink-0 items-center gap-3 border-b border-line bg-surface px-4">
        <div className="flex w-24 shrink-0 items-center gap-1.5 text-accent">
          <PawIcon className="h-[18px] w-[18px]" />
          <span className="text-title font-semibold tracking-tight text-ink">Pet</span>
        </div>

        {/* Underlined tabs — the app's primary navigation */}
        <nav className="flex min-w-0 flex-1 justify-center">
          {tabs.map((tab) => {
            const active = tab.value === activeTab;
            return (
              <button
                key={tab.value}
                onClick={() => setActiveTab(tab.value)}
                className={`relative px-4 py-3 text-body font-medium transition-colors ${
                  active ? "text-accent" : "text-ink-soft hover:text-ink"
                }`}
              >
                {tab.label}
                {active && <span className="absolute inset-x-3 bottom-0 h-[2px] rounded-full bg-accent" />}
              </button>
            );
          })}
        </nav>

        <div className="flex w-24 shrink-0 justify-end">
          <IconActionButton onClick={openDebugWindow} title={t("panel.openDebug")}>
            <BugIcon className="h-[18px] w-[18px]" />
          </IconActionButton>
        </div>
      </header>

      {/* Tab content */}
      <div className="min-h-0 flex-1 overflow-hidden">
        {activeTab === "settings" && <PanelSettings />}
        {activeTab === "chat" && <PanelChat />}
        {activeTab === "group" && <PanelGroup />}
        {activeTab === "tasks" && <PanelTasks />}
      </div>
    </div>
  );
}
