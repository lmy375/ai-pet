import { useEffect, useMemo, useState } from "react";
import { useSwarm, type SwarmTask } from "../../hooks/useSwarm";
import { useSettings } from "../../hooks/useSettings";
import { useI18n } from "../../i18n";
import { ChatThread } from "../ChatThread";
import { ChatInput } from "../ChatInput";
import { Button } from "../ui/Button";
import { Select } from "../ui/fields";
import { Segmented } from "../ui/Segmented";
import { LoadingScreen } from "../ui/feedback";
import { StatusText } from "../ui/StatusText";
import { ChevronRight, CloseIcon, PlusIcon, TrashIcon } from "../Icons";
import { ResizeHandle } from "../ui/ResizeHandle";
import { IconActionButton } from "../ui/IconButton";
import { useResizableWidth } from "../../hooks/useResizableWidth";
import { useStatusMessage } from "../../hooks/useStatusMessage";
import { ask } from "@tauri-apps/plugin-dialog";
import { TaskCard, TASK_DOT, TASK_GROUPS, TASK_LABEL, taskGroup, type TaskGroup } from "./swarm/TaskCard";
import { CreateProjectDialog, CreateTaskDialog } from "./swarm/dialogs";
import { ProjectSettings } from "./swarm/ProjectSettings";

type Conversation = { kind: "orchestrator" } | { kind: "worker"; id: string };
type LeftTab = "board" | "settings";

export function PanelSwarm() {
  const { t } = useI18n();
  const { settings, loaded } = useSettings();
  const swarm = useSwarm();
  const [conversation, setConversation] = useState<Conversation>({ kind: "orchestrator" });
  const [leftTab, setLeftTab] = useState<LeftTab>("board");
  const [newProjectOpen, setNewProjectOpen] = useState(false);
  const [newTaskOpen, setNewTaskOpen] = useState(false);
  const status = useStatusMessage();
  const [openWorkerIds, setOpenWorkerIds] = useState<string[]>([]);
  const [collapsedGroups, setCollapsedGroups] = useState<Set<TaskGroup>>(new Set());
  const projectPane = useResizableWidth({
    storageKey: "swarm-project",
    initialWidth: 420,
    minWidth: 320,
    maxWidth: 760,
  });

  useEffect(() => {
    setConversation({ kind: "orchestrator" });
    setOpenWorkerIds([]);
    setCollapsedGroups(new Set());
    setLeftTab("board");
    status.clear();
  }, [swarm.snapshot?.id]);

  const workers = swarm.snapshot?.workers ?? [];
  const activeView = conversation.kind === "orchestrator" ? swarm.mainView : swarm.workerViews[conversation.id];
  const groups = useMemo(() => {
    const tasks = swarm.snapshot?.tasks ?? [];
    return {
      pending: tasks.filter((task) => taskGroup(task) === "pending"),
      running: tasks.filter((task) => taskGroup(task) === "running"),
      error: tasks.filter((task) => taskGroup(task) === "error"),
      completed: tasks.filter((task) => taskGroup(task) === "completed"),
    };
  }, [swarm.snapshot?.tasks]);

  if (!loaded) return <LoadingScreen />;

  const createProject = async (title: string, orchestratorId: string) => {
    await swarm.create(title, orchestratorId);
    setNewProjectOpen(false);
  };

  if (!swarm.snapshot) {
    return (
      <div className="flex h-full items-center justify-center bg-canvas px-6">
        <div className="max-w-[420px] text-center">
          <div className="text-title font-semibold text-ink">{t("swarm.noProjects")}</div>
          <div className="mt-2 text-body text-ink-soft">{t("swarm.noProjectsHint")}</div>
          <Button className="mt-5" onClick={() => setNewProjectOpen(true)}>{t("swarm.project.create")}</Button>
        </div>
        {newProjectOpen && (
          <CreateProjectDialog
            agents={settings.agents}
            defaultAgentId={settings.active_agent}
            onCreate={createProject}
            onClose={() => setNewProjectOpen(false)}
          />
        )}
      </div>
    );
  }

  const snapshot = swarm.snapshot;
  const totalWorkers = snapshot.pool.reduce((sum, item) => sum + item.count, 0);
  const agentName = (id: string) => settings.agents.find((agent) => agent.id === id)?.name ?? id;
  const workerForTask = (taskId: string) =>
    [...workers].reverse().find((worker) => worker.task_id === taskId);

  const openWorkerChat = (workerId: string) => {
    setOpenWorkerIds((ids) => ids.includes(workerId) ? ids : [...ids, workerId]);
    setConversation({ kind: "worker", id: workerId });
  };

  const closeWorkerTab = (workerId: string) => {
    setOpenWorkerIds((ids) => ids.filter((id) => id !== workerId));
    setConversation((current) => current.kind === "worker" && current.id === workerId
      ? { kind: "orchestrator" }
      : current);
  };

  const toggleTaskGroup = (group: TaskGroup) => {
    setCollapsedGroups((groups) => {
      const next = new Set(groups);
      if (next.has(group)) next.delete(group);
      else next.add(group);
      return next;
    });
  };

  const runAction = async (action: () => Promise<void>) => {
    status.clear();
    try {
      await action();
    } catch (e) {
      status.fail(String(e));
    }
  };

  const deleteTask = async (task: SwarmTask) => {
    if (!(await ask(t("swarm.task.confirmDelete", { title: task.title })))) return;
    await runAction(() => swarm.deleteTask(task.id));
  };

  const deleteProject = async () => {
    if (!(await ask(t("swarm.project.confirmDelete", { title: snapshot.title })))) return;
    await runAction(swarm.deleteSwarm);
  };

  return (
    <div className="flex h-full min-h-0 w-full bg-canvas">
      <aside style={{ width: projectPane.width }} className="flex min-h-0 shrink-0 flex-col bg-surface">
        <div className="flex shrink-0 items-center gap-2 border-b border-line px-3 py-2">
          <Select
            size="sm"
            className="min-w-0 flex-1"
            value={swarm.activeId}
            onChange={swarm.setActiveId}
            options={swarm.swarms.map((item) => ({ value: item.id, label: item.title }))}
          />
          <Button size="sm" variant="ghost" onClick={() => setNewProjectOpen(true)}>{t("swarm.project.new")}</Button>
          <IconActionButton
            variant="danger"
            aria-label={t("swarm.project.delete")}
            title={t("swarm.project.delete")}
            onClick={deleteProject}
          >
            <TrashIcon className="h-4 w-4" />
          </IconActionButton>
        </div>

        <div className="flex shrink-0 border-b border-line px-3 py-2">
          <Segmented<LeftTab>
            value={leftTab}
            options={[
              { value: "board", label: t("swarm.board") },
              { value: "settings", label: t("swarm.settings") },
            ]}
            onChange={setLeftTab}
          />
        </div>

        {status.message && (
          <div className="shrink-0 border-b border-line px-3 py-1.5">
            <StatusText ok={status.message.ok} className="text-note">{status.message.text}</StatusText>
          </div>
        )}

        {leftTab === "board" ? (
          <div className="min-h-0 flex-1 overflow-y-auto p-3">
            <div className="mb-2 flex items-center justify-between">
              <div className="text-note font-semibold text-ink">{t("swarm.tasks")} <span className="font-normal text-ink-faint">{snapshot.tasks.length}</span></div>
              <Button size="sm" onClick={() => setNewTaskOpen(true)}>
                <PlusIcon className="h-4 w-4" />
                {t("swarm.task.new")}
              </Button>
            </div>
            <div className="mb-3 flex flex-wrap items-center gap-1.5 rounded-field bg-surface-soft px-2 py-2">
              <span className="rounded-full bg-accent-soft px-2 py-1 text-meta text-accent">{t(`swarm.status.${snapshot.status}`)}</span>
              {snapshot.status !== "running" && snapshot.status !== "draining" && (
                <Button size="sm" disabled={groups.pending.length === 0} onClick={() => runAction(swarm.start)}>{t("swarm.start")}</Button>
              )}
              {snapshot.status === "running" && (
                <Button size="sm" variant="ghost" onClick={() => runAction(swarm.end)}>{t("swarm.end")}</Button>
              )}
              {(snapshot.status === "running" || snapshot.status === "draining") && (
                <Button size="sm" variant="danger" onClick={() => runAction(swarm.stop)}>{t("swarm.stop")}</Button>
              )}
              <span className="ml-auto text-meta text-ink-faint">{totalWorkers} {t("swarm.workers")}</span>
            </div>
            {TASK_GROUPS.map((group) => {
              const collapsed = collapsedGroups.has(group);
              return (
                <div key={group} className="mb-3">
                  <button
                    type="button"
                    aria-expanded={!collapsed}
                    onClick={() => toggleTaskGroup(group)}
                    className="mb-1.5 flex w-full items-center gap-1.5 text-left text-meta font-medium text-ink-faint hover:text-ink"
                  >
                    <ChevronRight className={`h-3.5 w-3.5 transition-transform ${collapsed ? "" : "rotate-90"}`} />
                    <span className={`h-2 w-2 shrink-0 rounded-full ${TASK_DOT[group]}`} />
                    {t(TASK_LABEL[group])} · {groups[group].length}
                  </button>
                  {!collapsed && (
                    <div className="flex flex-col gap-2">
                      {groups[group].map((task) => {
                        const worker = workerForTask(task.id);
                        return <TaskCard key={task.id} task={task} workerId={worker?.id} onOpenChat={openWorkerChat} onDelete={deleteTask} />;
                      })}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        ) : (
          <div className="min-h-0 flex-1 overflow-y-auto">
            <ProjectSettings snapshot={snapshot} settings={settings} onSave={swarm.configure} />
          </div>
        )}
      </aside>

      <ResizeHandle
        side="left"
        width={projectPane.width}
        minWidth={320}
        maxWidth={760}
        onChange={projectPane.setWidth}
        label={t("common.resizePanel")}
      />

      <section className="flex min-w-0 flex-1 flex-col">
        <div className="flex shrink-0 items-center gap-2 border-b border-line bg-surface px-4 py-2">
          <div className="flex min-w-0 flex-1 items-center gap-1 overflow-x-auto">
            <div className={`flex shrink-0 items-center rounded-field border ${
              conversation.kind === "orchestrator" ? "border-accent-line bg-accent-soft" : "border-line bg-surface-soft"
            }`}>
              <button
                type="button"
                onClick={() => setConversation({ kind: "orchestrator" })}
                className={`px-2.5 py-1 text-note font-medium ${conversation.kind === "orchestrator" ? "text-accent" : "text-ink-soft hover:text-ink"}`}
              >
                Orchestrator · {agentName(snapshot.main_agent_id)}
              </button>
            </div>
            {openWorkerIds.map((workerId) => {
              const worker = workers.find((item) => item.id === workerId);
              const task = worker?.task_id ? snapshot.tasks.find((item) => item.id === worker.task_id) : undefined;
              const active = conversation.kind === "worker" && conversation.id === workerId;
              const label = worker
                ? `${agentName(worker.agent_id)} · ${task?.title ?? worker.task_id?.slice(0, 8) ?? ""}`
                : workerId;
              return (
                <div
                  key={workerId}
                  className={`flex shrink-0 items-center rounded-field border ${
                    active ? "border-accent-line bg-accent-soft" : "border-line bg-surface-soft"
                  }`}
                >
                  <button
                    type="button"
                    onClick={() => setConversation({ kind: "worker", id: workerId })}
                    className={`max-w-[220px] truncate px-2.5 py-1 text-note font-medium ${
                      active ? "text-accent" : "text-ink-soft hover:text-ink"
                    }`}
                  >
                    {label}
                  </button>
                  <button
                    type="button"
                    onClick={() => closeWorkerTab(workerId)}
                    aria-label={t("swarm.task.closeChat")}
                    title={t("swarm.task.closeChat")}
                    className="flex h-6 w-6 items-center justify-center rounded-full text-ink-faint hover:bg-hover hover:text-ink"
                  >
                    <CloseIcon className="h-3.5 w-3.5" />
                  </button>
                </div>
              );
            })}
          </div>
        </div>
        <ChatThread
          items={activeView.items}
          currentToolCalls={activeView.toolCalls}
          streaming={activeView.streaming}
          streamingReasoning={activeView.reasoning}
          loading={activeView.running || (conversation.kind === "orchestrator" && swarm.mainRunning)}
          className="min-h-0 flex-1 px-5 py-4"
          emptyHint={conversation.kind === "orchestrator" ? t("swarm.orchestratorEmpty") : t("swarm.workerEmpty")}
          assistantName={conversation.kind === "orchestrator" ? "Orchestrator" : `Worker · ${agentName(workers.find((worker) => worker.id === conversation.id)?.agent_id ?? "")}`}
        />
        {conversation.kind === "orchestrator" && (
          <div className="shrink-0 border-t border-line bg-surface px-4 py-3">
            <ChatInput onSend={swarm.sendMain} isLoading={swarm.mainRunning} onStop={swarm.cancelMain} placeholder={t("swarm.orchestratorPlaceholder")} />
          </div>
        )}
      </section>

      {newProjectOpen && (
        <CreateProjectDialog
          agents={settings.agents}
          defaultAgentId={settings.active_agent}
          onCreate={createProject}
          onClose={() => setNewProjectOpen(false)}
        />
      )}
      {newTaskOpen && (
        <CreateTaskDialog
          onCreate={async (title, detail) => {
            await swarm.createTask(title, detail);
            setNewTaskOpen(false);
          }}
          onClose={() => setNewTaskOpen(false)}
        />
      )}
    </div>
  );
}