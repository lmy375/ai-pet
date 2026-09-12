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
    runningSessions,
    sendMessage,
    stopStreaming,
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
        running={runningSessions}
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
          <ContextUsageRing usage={contextUsage} />
          {items.length > 0 && !selectionMode && !isLoading && (
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
            <ChatInput onSend={sendMessage} isLoading={isLoading} onStop={stopStreaming} />
          </div>
        )}
      </div>
    </div>
  );
}

/* ---------- Context-usage gauge ---------- */

interface ToolInfo {
  name: string;
  description: string;
  is_mcp: boolean;
}

/**
 * The context-occupancy gauge in the chat toolbar: a ring + percentage that opens
 * a popover with the token breakdown and the live tool list. Hovering opens it,
 * clicking pins it open (so the tool list can be scrolled); Esc / outside-click
 * closes a pinned one.
 *
 * It stays mounted even with no usage numbers — a provider that reports no token
 * usage (or a session last written by the CLI) would otherwise take the whole
 * tool list away with it. The ring then reads empty and the percentage "—".
 */
function ContextUsageRing({ usage }: { usage: { used: number; total: number } | null }) {
  const { t } = useI18n();
  const { settings } = useSettings();
  const [pinned, setPinned] = useState(false);
  const [hovered, setHovered] = useState(false);
  const [tools, setTools] = useState<ToolInfo[] | null>(null);
  const ref = useRef<HTMLDivElement>(null);
  // Hover-out is delayed so crossing into the popover doesn't close it.
  const leaveTimer = useRef<number | undefined>(undefined);

  const open = pinned || hovered;

  const onEnter = () => {
    window.clearTimeout(leaveTimer.current);
    setHovered(true);
  };
  const onLeave = () => {
    window.clearTimeout(leaveTimer.current);
    leaveTimer.current = window.setTimeout(() => setHovered(false), 150);
  };
  useEffect(() => () => window.clearTimeout(leaveTimer.current), []);

  useEffect(() => {
    if (!pinned) return;
    const onDown = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setPinned(false);
    };
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && setPinned(false);
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [pinned]);

  // Fetch the live tool list each time the popover opens (MCP tools can change).
  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    invoke<ToolInfo[]>("list_available_tools", { agentId: settings.active_agent })
      .then((list) => !cancelled && setTools(list))
      .catch(() => !cancelled && setTools([]));
    return () => { cancelled = true; };
  }, [open, settings.active_agent]);

  const known = !!usage && usage.total > 0;
  const used = usage?.used ?? 0;
  const total = usage?.total ?? 0;
  const ratio = known ? used / total : 0;
  const percent = Math.round(ratio * 100);
  const remaining = Math.max(0, total - used);
  const tip = known
    ? t("chat.context.tooltip", {
        used: used.toLocaleString(),
        total: total.toLocaleString(),
        percent,
      })
    : t("chat.context.none");

  return (
    <div className="relative" ref={ref} onMouseEnter={onEnter} onMouseLeave={onLeave}>
      <button
        type="button"
        onClick={() => setPinned((v) => !v)}
        title={tip}
        aria-label={tip}
        className={`flex items-center gap-1.5 rounded-field border px-2 py-1 transition-colors ${
          open ? "border-accent-line bg-accent-soft" : "border-line bg-surface hover:bg-hover"
        }`}
      >
        <ProgressRing value={ratio} />
        <span className={`text-note font-medium ${known ? "text-ink-soft" : "text-ink-faint"}`}>
          {known ? `${percent}%` : "—"}
        </span>
      </button>
      {open && (
        /* Anchored flush to the button so the hover path into it is unbroken. */
        <div className="absolute right-0 top-full z-30 pt-1.5">
          <div className="w-64 rounded-card border border-line bg-surface p-3 text-left shadow-pop">
            <div className="flex items-baseline justify-between">
              <span className="text-note font-semibold text-ink-soft">{t("chat.context.title")}</span>
              <span className="text-title font-semibold text-accent">{known ? `${percent}%` : "—"}</span>
            </div>
            {known ? (
              <>
                <div className="mt-1.5 text-note text-ink-soft">
                  {t("chat.context.usedTotal", { used: used.toLocaleString(), total: total.toLocaleString() })}
                </div>
                <div className="mt-0.5 text-note text-ink-faint">
                  {t("chat.context.remaining", { remaining: remaining.toLocaleString() })}
                </div>
              </>
            ) : (
              <div className="mt-1.5 text-note text-ink-faint">{t("chat.context.none")}</div>
            )}

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
        </div>
      )}
    </div>
  );
}
