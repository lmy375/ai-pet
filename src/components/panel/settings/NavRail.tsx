import type { ComponentType, ReactNode } from "react";
import type { AgentConfig } from "../../../hooks/useSettings";
import { useI18n } from "../../../i18n";
import { AgentIcon, PlusIcon } from "../../Icons";
import { AGENT_PREFIX, GENERAL_ITEMS, NAV_GROUPS, type TabId } from "./nav";

/** The settings page's left rail: fixed groups, one tab per agent, general last. */
export function NavRail({
  width,
  tab,
  agents,
  activeAgentId,
  onSelect,
  onAddAgent,
}: {
  width: number;
  tab: TabId;
  agents: AgentConfig[];
  activeAgentId: string;
  onSelect: (tab: TabId) => void;
  onAddAgent: () => void;
}) {
  const { t } = useI18n();

  return (
    <aside style={{ width }} className="flex min-h-0 shrink-0 flex-col overflow-y-auto bg-canvas px-2.5 py-3">
      {NAV_GROUPS.map((group, gi) => (
        <div key={group.title} className={gi === 0 ? "" : "mt-4"}>
          <div className="px-3 pb-1.5 text-meta font-semibold text-ink-faint">{t(group.title)}</div>
          <div className="flex flex-col gap-0.5">
            {group.items.map((entry) => (
              <NavItem key={entry.id} active={tab === entry.id} icon={entry.icon} onClick={() => onSelect(entry.id)}>
                {t(entry.label)}
              </NavItem>
            ))}
          </div>
        </div>
      ))}

      {/* Agents: one tab per agent; "+" adds one. */}
      <div className="mt-4">
        <div className="flex items-center justify-between px-3 pb-1.5">
          <span className="text-meta font-semibold text-ink-faint">{t("settings.group.agents")}</span>
          <button
            onClick={onAddAgent}
            title={t("settings.agent.add")}
            className="flex h-5 w-5 items-center justify-center rounded-md text-ink-faint transition-colors hover:bg-hover hover:text-ink"
          >
            <PlusIcon className="h-3.5 w-3.5" />
          </button>
        </div>
        <div className="flex flex-col gap-0.5">
          {agents.map((a) => (
            <NavItem
              key={a.id}
              active={tab === `${AGENT_PREFIX}${a.id}`}
              icon={AgentIcon}
              onClick={() => onSelect(`${AGENT_PREFIX}${a.id}`)}
              dot={a.id === activeAgentId}
            >
              {a.name}
            </NavItem>
          ))}
        </div>
      </div>

      <div className="mt-4">
        <div className="px-3 pb-1.5 text-meta font-semibold text-ink-faint">{t("settings.group.general")}</div>
        <div className="flex flex-col gap-0.5">
          {GENERAL_ITEMS.map((entry) => (
            <NavItem key={entry.id} active={tab === entry.id} icon={entry.icon} onClick={() => onSelect(entry.id)}>
              {t(entry.label)}
            </NavItem>
          ))}
        </div>
      </div>
    </aside>
  );
}

/* ---------- Left-rail tab button ---------- */

function NavItem({
  active,
  icon: Icon,
  onClick,
  children,
  dot = false,
}: {
  active: boolean;
  icon: ComponentType<{ className?: string }>;
  onClick: () => void;
  children: ReactNode;
  dot?: boolean;
}) {
  return (
    <button
      onClick={onClick}
      className={`flex w-full items-center gap-2.5 rounded-lg px-3 py-1.5 text-left text-body transition-colors ${
        active ? "bg-surface font-medium text-ink shadow-card" : "text-ink-soft hover:bg-hover"
      }`}
    >
      <Icon className={`h-4 w-4 shrink-0 ${active ? "text-accent" : "text-ink-faint"}`} />
      <span className="min-w-0 flex-1 truncate">{children}</span>
      {dot && <span className="h-1.5 w-1.5 shrink-0 rounded-full bg-accent" />}
    </button>
  );
}