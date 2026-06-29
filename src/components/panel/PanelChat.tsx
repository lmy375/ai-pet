import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useChat, DEFAULT_SESSION_TITLE } from "../../hooks/useChat";
import { ChatThread } from "../ChatThread";
import { ChatInput } from "../ChatInput";
import { Button } from "../ui/Button";
import { ProgressRing } from "../ui/ProgressRing";
import { IconActionButton } from "../ui/IconButton";
import { LoadingScreen } from "../ui/feedback";
import { ExpandChevron, PlusIcon, PencilIcon, TrashIcon, CheckIcon } from "../Icons";
import { AgentSwitcher } from "../AgentSwitcher";
import { useSettings } from "../../hooks/useSettings";
import { useI18n } from "../../i18n";

export function PanelChat() {
  const { t } = useI18n();
  const {
    items,
    isLoading,
    currentResponse,
    currentToolCalls,
    loaded,
    contextUsage,
    sessionId,
    sessionTitle,
    sessionList,
    sendMessage,
    newSession,
    renameSession,
    switchSession,
    deleteSession,
    deleteItems,
  } = useChat();

  const [showSessionList, setShowSessionList] = useState(false);
  // Inline rename in the session list: id of the row being edited + its draft.
  const [editingId, setEditingId] = useState<string | null>(null);
  const [titleDraft, setTitleDraft] = useState("");
  // Set on Escape so the input's blur cancels instead of saving.
  const cancelRenameRef = useRef(false);

  // Multi-select mode for deleting messages. `selected` holds indices into `items`;
  // `confirming` is the two-step-delete guard (first click arms, second executes).
  const [selectionMode, setSelectionMode] = useState(false);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [confirming, setConfirming] = useState(false);

  const exitSelection = () => {
    setSelectionMode(false);
    setSelected(new Set());
    setConfirming(false);
  };
  const toggleSelect = (i: number) => {
    setConfirming(false);
    setSelected((prev) => {
      const next = new Set(prev);
      next.has(i) ? next.delete(i) : next.add(i);
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

  // Untitled sessions are stored with the sentinel title; show a localized label.
  const displayTitle = (title: string) => (title === DEFAULT_SESSION_TITLE ? t("chat.newSession") : title);

  const startRename = (id: string, title: string) => {
    setTitleDraft(title === DEFAULT_SESSION_TITLE ? "" : title);
    setEditingId(id);
  };
  const commitRename = () => {
    const id = editingId;
    setEditingId(null);
    if (cancelRenameRef.current) {
      cancelRenameRef.current = false;
      return;
    }
    if (id) renameSession(id, titleDraft);
  };

  if (!loaded) {
    return <LoadingScreen />;
  }

  return (
    <div className="flex h-full flex-col bg-slate-100">
      {/* Session header bar */}
      <div className="flex shrink-0 items-center gap-2 border-b border-slate-200/70 bg-white px-4 py-2">
        <AgentSwitcher />
        <button
          className="flex min-w-0 flex-1 items-center gap-1.5"
          onClick={() => setShowSessionList(!showSessionList)}
        >
          <span className="truncate text-[13px] font-semibold text-slate-800">{displayTitle(sessionTitle)}</span>
          <ExpandChevron expanded={showSessionList} />
        </button>
        {contextUsage && contextUsage.total > 0 && (
          <ContextUsageRing used={contextUsage.used} total={contextUsage.total} />
        )}
        {items.length > 0 && !selectionMode && (
          <Button variant="ghost" size="sm" onClick={() => setSelectionMode(true)} title={t("chat.select.enter")}>
            <CheckIcon className="h-4 w-4" />
            {t("chat.select.enter")}
          </Button>
        )}
        <Button variant="ghost" size="sm" onClick={() => { newSession(); setShowSessionList(false); }} title={t("chat.session.newTitle")}>
          <PlusIcon className="h-4 w-4" />
          {t("chat.newSession")}
        </Button>
      </div>

      {/* Session list dropdown */}
      {showSessionList && (
        <div className="max-h-60 shrink-0 overflow-y-auto border-b border-slate-200/70 bg-white">
          {sessionList.length === 0 ? (
            <div className="py-3 text-center text-[12px] text-slate-400">{t("chat.session.empty")}</div>
          ) : (
            [...sessionList].reverse().map((s) => (
              <div
                key={s.id}
                className={`flex items-center gap-2 border-b border-slate-100 px-3 py-2 ${
                  s.id === sessionId ? "bg-sky-50" : ""
                }`}
              >
                {editingId === s.id ? (
                  <input
                    autoFocus
                    value={titleDraft}
                    onChange={(e) => setTitleDraft(e.target.value)}
                    onFocus={(e) => e.currentTarget.select()}
                    onBlur={commitRename}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") e.currentTarget.blur();
                      else if (e.key === "Escape") { cancelRenameRef.current = true; e.currentTarget.blur(); }
                    }}
                    placeholder={t("chat.session.titlePlaceholder")}
                    className="min-w-0 flex-1 rounded-md border border-accent bg-white px-2 py-1 text-[13px] text-slate-800 focus:outline-none"
                  />
                ) : (
                  <button
                    className="min-w-0 flex-1 text-left"
                    onClick={() => { switchSession(s.id); setShowSessionList(false); }}
                  >
                    <div className={`truncate text-[13px] text-slate-800 ${s.id === sessionId ? "font-semibold" : ""}`}>
                      {displayTitle(s.title)}
                    </div>
                    <div className="text-[11px] text-slate-400">{s.updated_at.split("T")[0]}</div>
                  </button>
                )}
                <IconActionButton
                  onClick={(e) => { e.stopPropagation(); startRename(s.id, s.title); }}
                  title={t("chat.session.rename")}
                >
                  <PencilIcon className="h-4 w-4" />
                </IconActionButton>
                <IconActionButton
                  variant="danger"
                  onClick={(e) => { e.stopPropagation(); deleteSession(s.id); }}
                  title={t("chat.session.delete")}
                >
                  <TrashIcon className="h-4 w-4" />
                </IconActionButton>
              </div>
            ))
          )}
        </div>
      )}

      {/* Message list */}
      <ChatThread
        items={items}
        currentToolCalls={currentToolCalls}
        streaming={currentResponse}
        loading={isLoading}
        className="flex-1 p-4"
        emptyHint={t("chat.empty")}
        selectionMode={selectionMode}
        selectedKeys={selected}
        onToggleSelect={toggleSelect}
      />

      {/* Selection action bar (replaces the input while choosing messages) */}
      {selectionMode ? (
        <div className="flex shrink-0 items-center justify-end gap-2 border-t border-slate-200/70 bg-white px-4 py-3">
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
        <div className="shrink-0 border-t border-slate-200/70 bg-white px-4 py-3">
          <ChatInput onSend={sendMessage} isLoading={isLoading} />
        </div>
      )}
    </div>
  );
}

/* ---------- Context-usage ring ---------- */

/**
 * The context-occupancy ring in the chat header. Hovering shows the summary via
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
        className="flex items-center justify-center rounded-md p-1 transition-colors hover:bg-slate-100"
      >
        <ProgressRing value={ratio} />
      </button>
      {open && (
        <div className="absolute right-0 top-full z-30 mt-1.5 w-64 rounded-xl border border-slate-200 bg-white p-3 text-left shadow-lg">
          <div className="flex items-baseline justify-between">
            <span className="text-[12px] font-semibold text-slate-700">{t("chat.context.title")}</span>
            <span className="text-[15px] font-semibold text-accent">{percent}%</span>
          </div>
          <div className="mt-1.5 text-[12px] text-slate-500">
            {t("chat.context.usedTotal", { used: used.toLocaleString(), total: total.toLocaleString() })}
          </div>
          <div className="mt-0.5 text-[12px] text-slate-400">
            {t("chat.context.remaining", { remaining: remaining.toLocaleString() })}
          </div>

          {/* Available tools */}
          <div className="mt-2.5 border-t border-slate-100 pt-2.5">
            <div className="mb-1.5 text-[11px] font-semibold uppercase tracking-wide text-slate-400">
              {t("chat.context.tools")}{tools ? ` (${tools.length})` : ""}
            </div>
            {tools === null ? (
              <div className="text-[12px] text-slate-400">{t("common.loading")}</div>
            ) : tools.length === 0 ? (
              <div className="text-[12px] text-slate-400">{t("chat.context.toolsEmpty")}</div>
            ) : (
              <div className="flex max-h-52 flex-col gap-0.5 overflow-y-auto">
                {tools.map((tool) => (
                  <div key={tool.name} className="rounded-md px-1.5 py-1 hover:bg-slate-50" title={tool.description}>
                    <div className="flex items-center gap-1.5">
                      <span className="truncate font-mono text-[12px] text-slate-700">{tool.name}</span>
                      {tool.is_mcp && (
                        <span className="shrink-0 rounded bg-purple-100 px-1 text-[10px] font-medium text-purple-600">MCP</span>
                      )}
                    </div>
                    {tool.description && (
                      <div className="truncate text-[11px] text-slate-400">{tool.description}</div>
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
