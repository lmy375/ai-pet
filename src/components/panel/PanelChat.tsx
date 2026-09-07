import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useChat } from "../../hooks/useChat";
import { ChatThread } from "../ChatThread";
import { ChatInput } from "../ChatInput";
import { SessionSidebar } from "./SessionSidebar";
import { Button } from "../ui/Button";
import { ProgressRing } from "../ui/ProgressRing";
import { LoadingScreen } from "../ui/feedback";
import { TrashIcon, CheckIcon } from "../Icons";
import { AgentSwitcher } from "../AgentSwitcher";
import { ModelSwitcher } from "../ModelSwitcher";
import { useSettings } from "../../hooks/useSettings";
import { useI18n } from "../../i18n";

export function PanelChat() {
  const { t } = useI18n();
  const { settings } = useSettings();
  const {
    items,
    isLoading,
    currentResponse,
    currentReasoning,
    currentToolCalls,
    loaded,
    contextUsage,
    sessionId,
    sessionList,
    sendMessage,
    newSession,
    renameSession,
    switchSession,
    deleteSession,
    deleteItems,
  } = useChat();

  // Multi-select mode for deleting messages. `selected` holds stable item ids
  // (not indices — items can shift); `confirming` is the two-step-delete guard.
  const [selectionMode, setSelectionMode] = useState(false);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [confirming, setConfirming] = useState(false);

  const exitSelection = () => {
    setSelectionMode(false);
    setSelected(new Set());
    setConfirming(false);
  };
  const toggleSelect = (id: string) => {
    setConfirming(false);
    setSelected((prev) => {
      const next = new Set(prev);
      next.has(id) ? next.delete(id) : next.add(id);
      return next;
    });
  };
  const handleDelete = async () => {
    if (selected.size === 0) return;
    if (!confirming) {
      setConfirming(true);
      return;
    }
    await deleteItems([...selected]);
    exitSelection();
  };

  // Leave selection mode whenever the session changes out from under us.
  useEffect(() => {
    exitSelection();
  }, [sessionId]);

  if (!loaded) {
    return <LoadingScreen />;
  }

  // The answering agent's name labels every assistant message.
  const petName = settings.agents.find((a) => a.id === settings.active_agent)?.name ?? "Pet";

  return (
    <div className="flex h-full min-h-0">
      <SessionSidebar
        sessions={sessionList}
        activeId={sessionId}
        onSelect={switchSession}
        onNew={newSession}
        onRename={renameSession}
        onDelete={deleteSession}
      />

      <div className="flex min-w-0 flex-1 flex-col bg-canvas">
        {/* Conversation toolbar: who answers, with what model, plus the
            context gauge and message selection. */}
        <div className="flex shrink-0 items-center gap-2 border-b border-line bg-surface px-4 py-2.5">
          <span className="shrink-0 text-note text-ink-faint">{t("chat.agentLabel")}</span>
          <AgentSwitcher />
          <span className="shrink-0 pl-1 text-note text-ink-faint">{t("chat.modelLabel")}</span>
          <ModelSwitcher className="max-w-[36%]" />
          <div className="min-w-0 flex-1" />
          {contextUsage && contextUsage.total > 0 && (
            <ContextUsageRing used={contextUsage.used} total={contextUsage.total} />
          )}
          {items.length > 0 && !selectionMode && (
            <Button variant="ghost" size="sm" onClick={() => setSelectionMode(true)} title={t("chat.select.enter")}>
              <CheckIcon className="h-4 w-4" />
              {t("chat.select.enter")}
            </Button>
          )}
        </div>

        {/* Message list */}
        <ChatThread
          items={items}
          currentToolCalls={currentToolCalls}
          streaming={currentResponse}
          streamingReasoning={currentReasoning}
          loading={isLoading}
          className="flex-1 px-5 py-4"
          emptyHint={t("chat.empty")}
          assistantName={petName}
          selectionMode={selectionMode}
          selectedKeys={selected}
          onToggleSelect={toggleSelect}
        />

        {/* Selection action bar (replaces the input while choosing messages) */}
        {selectionMode ? (
          <div className="flex shrink-0 items-center justify-end gap-2 border-t border-line bg-surface px-4 py-3">
            <Button variant="ghost" size="sm" onClick={exitSelection}>
              {t("chat.select.cancel")}
            </Button>
            <Button variant="danger" size="sm" disabled={selected.size === 0} onClick={handleDelete}>
              <TrashIcon className="h-4 w-4" />
              {confirming
                ? t("chat.select.confirm", { count: selected.size })
                : t("chat.select.delete", { count: selected.size })}
            </Button>
          </div>
        ) : (
          /* Input bar */
          <div className="shrink-0 border-t border-line bg-surface px-4 py-3">
            <ChatInput onSend={sendMessage} isLoading={isLoading} />
          </div>
        )}
      </div>
    </div>
  );
}

/* ---------- Context-usage ring ---------- */

/**
 * The context-occupancy ring in the chat toolbar. Hovering shows the summary via
 * the native tooltip; clicking toggles a small popover with the same detail
 * (more discoverable, and works without a pointer). Closes on outside-click/Esc.
 */
interface ToolInfo {
  name: string;
  description: string;
  is_mcp: boolean;
}

function ContextUsageRing({ used, total }: { used: number; total: number }) {
  const { t } = useI18n();
  const { settings } = useSettings();
  const [open, setOpen] = useState(false);
  const [tools, setTools] = useState<ToolInfo[] | null>(null);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  // Fetch the live tool list each time the popover opens (MCP tools can change).
  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    invoke<ToolInfo[]>("list_available_tools", { agentId: settings.active_agent })
      .then((list) => !cancelled && setTools(list))
      .catch(() => !cancelled && setTools([]));
    return () => { cancelled = true; };
  }, [open, settings.active_agent]);

  const ratio = total > 0 ? used / total : 0;
  const percent = Math.round(ratio * 100);
  const remaining = Math.max(0, total - used);
  const tip = t("chat.context.tooltip", {
    used: used.toLocaleString(),
    total: total.toLocaleString(),
    percent,
  });

  return (
    <div className="relative" ref={ref}>
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        title={tip}
        aria-label={tip}
        className="flex items-center justify-center rounded-md p-1 transition-colors hover:bg-hover"
      >
        <ProgressRing value={ratio} />
      </button>
      {open && (
        <div className="absolute right-0 top-full z-30 mt-1.5 w-64 rounded-card border border-line bg-surface p-3 text-left shadow-pop">
          <div className="flex items-baseline justify-between">
            <span className="text-note font-semibold text-ink-soft">{t("chat.context.title")}</span>
            <span className="text-title font-semibold text-accent">{percent}%</span>
          </div>
          <div className="mt-1.5 text-note text-ink-soft">
            {t("chat.context.usedTotal", { used: used.toLocaleString(), total: total.toLocaleString() })}
          </div>
          <div className="mt-0.5 text-note text-ink-faint">
            {t("chat.context.remaining", { remaining: remaining.toLocaleString() })}
          </div>

          {/* Available tools */}
          <div className="mt-2.5 border-t border-line pt-2.5">
            <div className="mb-1.5 text-meta font-semibold uppercase tracking-wide text-ink-faint">
              {t("chat.context.tools")}{tools ? ` (${tools.length})` : ""}
            </div>
            {tools === null ? (
              <div className="text-note text-ink-faint">{t("common.loading")}</div>
            ) : tools.length === 0 ? (
              <div className="text-note text-ink-faint">{t("chat.context.toolsEmpty")}</div>
            ) : (
              <div className="flex max-h-52 flex-col gap-0.5 overflow-y-auto">
                {tools.map((tool) => (
                  <div key={tool.name} className="rounded-md px-1.5 py-1 hover:bg-surface-soft" title={tool.description}>
                    <div className="flex items-center gap-1.5">
                      <span className="truncate font-mono text-note text-ink">{tool.name}</span>
                      {tool.is_mcp && (
                        <span className="shrink-0 rounded bg-purple-100 px-1 text-meta font-medium text-purple-600">MCP</span>
                      )}
                    </div>
                    {tool.description && (
                      <div className="truncate text-meta text-ink-faint">{tool.description}</div>
                    )}
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
