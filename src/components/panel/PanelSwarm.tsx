import { useEffect, useMemo, useState } from "react";
import { useSwarm, type SwarmConfig, type SwarmSnapshot, type SwarmTask } from "../../hooks/useSwarm";
import { useSettings, type AgentConfig, type AppSettings } from "../../hooks/useSettings";
import { useI18n } from "../../i18n";
import { ChatThread } from "../ChatThread";
import { ChatInput } from "../ChatInput";
import { Button } from "../ui/Button";
import { Segmented } from "../ui/Segmented";
import { LoadingScreen } from "../ui/feedback";
import { ChevronRight, CloseIcon, PlusIcon } from "../Icons";
import { ResizeHandle } from "../ui/ResizeHandle";
import { useResizableWidth } from "../../hooks/useResizableWidth";

type Conversation = { kind: "orchestrator" } | { kind: "worker"; id: string };
type LeftTab = "board" | "settings";
type TaskGroup = "pending" | "running" | "completed";

const TASK_GROUPS: TaskGroup[] = ["pending", "running", "completed"];

function TaskCard({
  task,
  workerId,
  onOpenChat,
}: {
  task: SwarmTask;
  workerId?: string;
  onOpenChat: (workerId: string) => void;
}) {
  const { t } = useI18n();
  const [expanded, setExpanded] = useState(false);
  const label = task.status === "pending"
    ? t("swarm.task.pending")
    : task.status === "running"
      ? t("swarm.task.running")
      : t("swarm.task.completed");
  return (
    <div className="rounded-card border border-line bg-surface p-2.5 shadow-sm">
      <div className="flex items-center gap-2">
        <div className="min-w-0 flex-1 truncate text-body font-medium text-ink">{task.title}</div>
        <span className="shrink-0 rounded-full bg-surface-soft px-2 py-0.5 text-meta text-ink-soft">{label}</span>
      </div>
      <div className="mt-2 flex justify-end gap-1.5">
        <Button variant="ghost" size="sm" onClick={() => setExpanded((value) => !value)}>
          {expanded ? t("swarm.task.hideDetails") : t("swarm.task.details")}
        </Button>
        <Button
          variant="ghost"
          size="sm"
          disabled={!workerId}
          title={workerId ? undefined : t("swarm.task.chatUnavailable")}
          onClick={() => workerId && onOpenChat(workerId)}
        >
          {t("swarm.task.chat")}
        </Button>
      </div>
      {expanded && (
        <div className="mt-2 border-t border-line/70 pt-2">
          <div className="whitespace-pre-wrap text-note text-ink-soft">{task.detail}</div>
          <div className="mt-2 h-1.5 overflow-hidden rounded-full bg-line">
            <div className="h-full rounded-full bg-accent transition-all" style={{ width: `${task.progress}%` }} />
          </div>
          <div className="mt-1 flex justify-between gap-2 text-meta text-ink-faint">
            <span className="truncate">{task.progress}% {task.progress_summary}</span>
            <span className="shrink-0">{task.attempt > 0 ? t("swarm.task.attempt", { count: task.attempt }) : ""}</span>
          </div>
          {task.result && <div className="mt-2 max-h-48 overflow-y-auto whitespace-pre-wrap rounded-field bg-surface-soft px-2 py-1.5 text-note text-ink-soft">{task.result}</div>}
        </div>
      )}
    </div>
  );
}

function CreateProjectDialog({
  agents,
  defaultAgentId,
  onCreate,
  onClose,
}: {
  agents: AgentConfig[];
  defaultAgentId: string;
  onCreate: (title: string, orchestratorId: string) => Promise<void>;
  onClose: () => void;
}) {
  const { t } = useI18n();
  const [title, setTitle] = useState("");
  const [orchestratorId, setOrchestratorId] = useState(defaultAgentId || agents[0]?.id || "");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  const submit = async () => {
    if (!title.trim() || !orchestratorId || busy) return;
    setBusy(true);
    setError("");
    try {
      await onCreate(title.trim(), orchestratorId);
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/25 px-4">
      <div className="w-full max-w-[440px] rounded-card border border-line bg-surface p-5 shadow-pop">
        <div className="text-title font-semibold text-ink">{t("swarm.project.create")}</div>
        <div className="mt-1 text-note text-ink-soft">{t("swarm.project.createHint")}</div>
        <label className="mt-4 block text-note font-medium text-ink">{t("swarm.project.name")}</label>
        <input
          autoFocus
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && submit()}
          placeholder={t("swarm.project.namePlaceholder")}
          className="mt-1 w-full rounded-field border border-line bg-surface-soft px-3 py-2 text-body text-ink outline-none focus:border-accent"
        />
        <label className="mt-4 block text-note font-medium text-ink">Orchestrator</label>
        <select
          value={orchestratorId}
          onChange={(e) => setOrchestratorId(e.target.value)}
          className="mt-1 w-full rounded-field border border-line bg-surface-soft px-3 py-2 text-body text-ink outline-none focus:border-accent"
        >
          {agents.map((agent) => <option key={agent.id} value={agent.id}>{agent.name}</option>)}
        </select>
        {error && <div className="mt-3 text-note text-red-500">{error}</div>}
        <div className="mt-5 flex justify-end gap-2">
          <Button variant="ghost" size="sm" onClick={onClose} disabled={busy}>{t("common.cancel")}</Button>
          <Button size="sm" onClick={submit} disabled={!title.trim() || !orchestratorId || busy}>{t("common.create")}</Button>
        </div>
      </div>
    </div>
  );
}

function CreateTaskDialog({ onCreate, onClose }: { onCreate: (title: string, detail: string) => Promise<void>; onClose: () => void }) {
  const { t } = useI18n();
  const [title, setTitle] = useState("");
  const [detail, setDetail] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  const submit = async () => {
    if (!title.trim() || !detail.trim() || busy) return;
    setBusy(true);
    setError("");
    try {
      await onCreate(title.trim(), detail.trim());
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/25 px-4">
      <div className="w-full max-w-[520px] rounded-card border border-line bg-surface p-5 shadow-pop">
        <div className="text-title font-semibold text-ink">{t("swarm.task.create")}</div>
        <input
          autoFocus
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          placeholder={t("swarm.task.title")}
          className="mt-4 w-full rounded-field border border-line bg-surface-soft px-3 py-2 text-body text-ink outline-none focus:border-accent"
        />
        <textarea
          value={detail}
          onChange={(e) => setDetail(e.target.value)}
          placeholder={t("swarm.task.detail")}
          rows={7}
          className="mt-2 w-full resize-y rounded-field border border-line bg-surface-soft px-3 py-2 text-body text-ink outline-none focus:border-accent"
        />
        {error && <div className="mt-3 text-note text-red-500">{error}</div>}
        <div className="mt-5 flex justify-end gap-2">
          <Button variant="ghost" size="sm" onClick={onClose} disabled={busy}>{t("common.cancel")}</Button>
          <Button size="sm" onClick={submit} disabled={!title.trim() || !detail.trim() || busy}>{t("common.create")}</Button>
        </div>
      </div>
    </div>
  );
}

function ProjectSettings({
  snapshot,
  settings,
  onSave,
}: {
  snapshot: SwarmSnapshot;
  settings: AppSettings;
  onSave: (config: SwarmConfig) => Promise<void>;
}) {
  const { t } = useI18n();
  const [form, setForm] = useState<SwarmConfig>({
    title: snapshot.title,
    main_agent_id: snapshot.main_agent_id,
    pool: snapshot.pool,
    shared_prompt: snapshot.shared_prompt,
    orchestrator_prompt: snapshot.orchestrator_prompt,
    worker_prompt: snapshot.worker_prompt,
  });
  const [message, setMessage] = useState("");
  const locked = snapshot.status === "running" || snapshot.status === "draining";

  useEffect(() => {
    setForm({
      title: snapshot.title,
      main_agent_id: snapshot.main_agent_id,
      pool: snapshot.pool,
      shared_prompt: snapshot.shared_prompt,
      orchestrator_prompt: snapshot.orchestrator_prompt,
      worker_prompt: snapshot.worker_prompt,
    });
  }, [snapshot.id]);

  const updatePool = (agentId: string, count: number) => {
    setForm((prev) => ({
      ...prev,
      pool: [...prev.pool.filter((item) => item.agent_id !== agentId), { agent_id: agentId, count }],
    }));
  };

  const save = async () => {
    if (!form.title.trim()) {
      setMessage(t("swarm.project.nameRequired"));
      return;
    }
    setMessage("");
    try {
      await onSave({ ...form, title: form.title.trim(), pool: form.pool.filter((item) => item.count > 0) });
      setMessage(t("common.saved"));
    } catch (e) {
      setMessage(String(e));
    }
  };

  return (
    <div className="flex flex-col">
      <div className="sticky top-0 z-10 flex items-center justify-between gap-3 border-b border-line bg-surface/95 px-4 py-3 backdrop-blur">
        <span className="min-w-0 truncate text-note text-ink-soft">{locked ? t("swarm.settings.locked") : message}</span>
        <Button size="sm" onClick={save} disabled={locked}>{t("common.save")}</Button>
      </div>
      <div className="flex flex-col gap-5 p-4">
        <section>
          <div className="mb-2 text-note font-semibold text-ink">{t("swarm.settings.project")}</div>
          <label className="text-meta text-ink-faint">{t("swarm.project.name")}</label>
          <input
            value={form.title}
            disabled={locked}
            onChange={(e) => setForm((prev) => ({ ...prev, title: e.target.value }))}
            className="mt-1 w-full rounded-field border border-line bg-surface px-3 py-2 text-body text-ink outline-none focus:border-accent disabled:bg-surface-soft disabled:text-ink-soft"
          />
          <label className="mt-3 block text-meta text-ink-faint">Orchestrator</label>
          <select
            value={form.main_agent_id}
            disabled={locked}
            onChange={(e) => setForm((prev) => ({ ...prev, main_agent_id: e.target.value }))}
            className="mt-1 w-full rounded-field border border-line bg-surface px-3 py-2 text-body text-ink outline-none focus:border-accent disabled:bg-surface-soft disabled:text-ink-soft"
          >
            {settings.agents.map((agent) => <option key={agent.id} value={agent.id}>{agent.name}</option>)}
          </select>
        </section>

        <section>
          <div className="mb-1 text-note font-semibold text-ink">{t("swarm.settings.workers")}</div>
          <div className="mb-2 text-meta text-ink-faint">{t("swarm.settings.inherited")}</div>
          <div className="flex flex-col gap-2">
            {settings.agents.map((agent) => {
              const count = form.pool.find((item) => item.agent_id === agent.id)?.count ?? 0;
              const model = settings.models[agent.model]?.model ?? agent.model;
              return (
                <div key={agent.id} className="flex items-center gap-3 rounded-field border border-line bg-surface px-3 py-2">
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-body text-ink">{agent.name}</div>
                    <div className="truncate text-meta text-ink-faint">{model || t("chat.model.none")}</div>
                  </div>
                  <input
                    type="number"
                    min={0}
                    max={64}
                    disabled={locked}
                    value={count}
                    onChange={(e) => updatePool(agent.id, Number(e.target.value) || 0)}
                    className="w-16 rounded-field border border-line bg-surface-soft px-2 py-1 text-body text-ink outline-none disabled:text-ink-soft"
                  />
                </div>
              );
            })}
          </div>
        </section>

        <section>
          <div className="mb-1 text-note font-semibold text-ink">{t("swarm.settings.sharedPrompt")}</div>
          <div className="mb-2 text-meta text-ink-faint">{t("swarm.settings.sharedPromptHint")}</div>
          <textarea
            value={form.shared_prompt}
            disabled={locked}
            onChange={(e) => setForm((prev) => ({ ...prev, shared_prompt: e.target.value }))}
            rows={6}
            className="w-full resize-y rounded-field border border-line bg-surface px-3 py-2 text-note text-ink outline-none focus:border-accent disabled:bg-surface-soft disabled:text-ink-soft"
          />
        </section>

        <section>
          <div className="mb-1 text-note font-semibold text-ink">{t("swarm.settings.orchestratorPrompt")}</div>
          <div className="mb-2 text-meta text-ink-faint">{t("swarm.settings.orchestratorPromptHint")}</div>
          <textarea
            value={form.orchestrator_prompt}
            disabled={locked}
            onChange={(e) => setForm((prev) => ({ ...prev, orchestrator_prompt: e.target.value }))}
            rows={5}
            className="w-full resize-y rounded-field border border-line bg-surface px-3 py-2 text-note text-ink outline-none focus:border-accent disabled:bg-surface-soft disabled:text-ink-soft"
          />
        </section>

        <section>
          <div className="mb-1 text-note font-semibold text-ink">{t("swarm.settings.workerPrompt")}</div>
          <div className="mb-2 text-meta text-ink-faint">{t("swarm.settings.workerPromptHint")}</div>
          <textarea
            value={form.worker_prompt}
            disabled={locked}
            onChange={(e) => setForm((prev) => ({ ...prev, worker_prompt: e.target.value }))}
            rows={5}
            className="w-full resize-y rounded-field border border-line bg-surface px-3 py-2 text-note text-ink outline-none focus:border-accent disabled:bg-surface-soft disabled:text-ink-soft"
          />
        </section>
      </div>
    </div>
  );
}

export function PanelSwarm() {
  const { t } = useI18n();
  const { settings, loaded } = useSettings();
  const swarm = useSwarm();
  const [conversation, setConversation] = useState<Conversation>({ kind: "orchestrator" });
  const [leftTab, setLeftTab] = useState<LeftTab>("board");
  const [newProjectOpen, setNewProjectOpen] = useState(false);
  const [newTaskOpen, setNewTaskOpen] = useState(false);
  const [notice, setNotice] = useState("");
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
    setNotice("");
  }, [swarm.snapshot?.id]);

  const workers = swarm.snapshot?.workers ?? [];
  const activeView = conversation.kind === "orchestrator" ? swarm.mainView : swarm.workerViews[conversation.id];
  const groups = useMemo(() => {
    const tasks = swarm.snapshot?.tasks ?? [];
    return {
      pending: tasks.filter((task) => task.status === "pending"),
      running: tasks.filter((task) => task.status === "running"),
      completed: tasks.filter((task) => task.status === "completed"),
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
    setNotice("");
    try {
      await action();
    } catch (e) {
      setNotice(String(e));
    }
  };

  return (
    <div className="flex h-full min-h-0 w-full bg-canvas">
      <aside style={{ width: projectPane.width }} className="flex min-h-0 shrink-0 flex-col bg-surface">
        <div className="flex shrink-0 items-center gap-2 border-b border-line px-3 py-2">
          <select
            value={swarm.activeId}
            onChange={(e) => swarm.setActiveId(e.target.value)}
            className="min-w-0 flex-1 rounded-field border border-line bg-surface-soft px-2 py-1.5 text-body text-ink"
          >
            {swarm.swarms.map((item) => <option key={item.id} value={item.id}>{item.title}</option>)}
          </select>
          <Button size="sm" variant="ghost" onClick={() => setNewProjectOpen(true)}>{t("swarm.project.new")}</Button>
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

        {notice && <div className="shrink-0 border-b border-line px-3 py-1.5 text-note text-red-500">{notice}</div>}

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
            {TASK_GROUPS.map((status) => {
              const collapsed = collapsedGroups.has(status);
              return (
                <div key={status} className="mb-3">
                  <button
                    type="button"
                    aria-expanded={!collapsed}
                    onClick={() => toggleTaskGroup(status)}
                    className="mb-1.5 flex w-full items-center gap-1 text-left text-meta font-medium text-ink-faint hover:text-ink"
                  >
                    <ChevronRight className={`h-3.5 w-3.5 transition-transform ${collapsed ? "" : "rotate-90"}`} />
                    {status === "pending" ? t("swarm.task.pending") : status === "running" ? t("swarm.task.running") : t("swarm.task.completed")} · {groups[status].length}
                  </button>
                  {!collapsed && (
                    <div className="flex flex-col gap-2">
                      {groups[status].map((task) => {
                        const worker = workerForTask(task.id);
                        return <TaskCard key={task.id} task={task} workerId={worker?.id} onOpenChat={openWorkerChat} />;
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
