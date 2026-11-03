import { useEffect, useState } from "react";
import { useChat } from "../../hooks/useChat";
import { ChatThread } from "../ChatThread";
import { ChatInput } from "../ChatInput";
import { ContextUsageRing } from "./ContextUsageRing";
import { SessionSidebar } from "./SessionSidebar";
import { TaskDrawer } from "./TaskDrawer";
import { Button } from "../ui/Button";
import { LoadingScreen } from "../ui/feedback";
import { TrashIcon, CheckIcon, TasksIcon } from "../Icons";
import { AgentSwitcher } from "../AgentSwitcher";
import { ModelSwitcher } from "../ModelSwitcher";
import { useSettings } from "../../hooks/useSettings";
import { useI18n } from "../../i18n";
import { useResizableWidth } from "../../hooks/useResizableWidth";
import { ResizeHandle } from "../ui/ResizeHandle";

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
    turnStartedAt,
    turnTokens,
    sessionId,
    sessionList,
    runningSessions,
    sendMessage,
    editMessage,
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
  const [tasksOpen, setTasksOpen] = useState(false);
  const sessionsPane = useResizableWidth({
    storageKey: "chat-sessions",
    initialWidth: 236,
    minWidth: 180,
    maxWidth: 440,
  });
  const tasksPane = useResizableWidth({
    storageKey: "chat-tasks",
    initialWidth: 360,
    minWidth: 280,
    maxWidth: 620,
  });

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

  useEffect(() => {
    if (!tasksOpen) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setTasksOpen(false);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [tasksOpen]);

  if (!loaded) {
    return <LoadingScreen />;
  }

  // The answering agent's name labels every assistant message.
  const petName = settings.agents.find((a) => a.id === settings.active_agent)?.name ?? "Pet";

  return (
    <div className="flex h-full min-h-0">
      <div style={{ width: sessionsPane.width }} className="flex min-h-0 shrink-0">
        <SessionSidebar
          sessions={sessionList}
          activeId={sessionId}
          running={runningSessions}
          onSelect={switchSession}
          onNew={newSession}
          onRename={renameSession}
          onDelete={deleteSession}
        />
      </div>
      <ResizeHandle
        side="left"
        width={sessionsPane.width}
        minWidth={180}
        maxWidth={440}
        onChange={sessionsPane.setWidth}
        label={t("common.resizePanel")}
      />

      <div className="flex min-w-0 flex-1">
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
            <Button
              variant="ghost"
              size="sm"
              onClick={() => setTasksOpen((open) => !open)}
              aria-expanded={tasksOpen}
              aria-controls="tasks-drawer"
              title={tasksOpen ? t("tasks.close") : t("tasks.open")}
            >
              <TasksIcon className="h-4 w-4" />
              {t("tasks.button")}
            </Button>
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
          turnStartedAt={turnStartedAt}
          turnTokens={turnTokens}
          className="flex-1 px-5 py-4"
          emptyHint={t("chat.empty")}
          assistantName={petName}
          selectionMode={selectionMode}
          selectedKeys={selected}
          onToggleSelect={toggleSelect}
          onEditMessage={editMessage}
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

        {tasksOpen && (
          <>
            <ResizeHandle
              side="right"
              width={tasksPane.width}
              minWidth={280}
              maxWidth={620}
              onChange={tasksPane.setWidth}
              label={t("common.resizePanel")}
            />
            <div style={{ width: tasksPane.width }} className="flex min-h-0 shrink-0">
              <TaskDrawer onClose={() => setTasksOpen(false)} />
            </div>
          </>
        )}
      </div>
    </div>
  );
}
