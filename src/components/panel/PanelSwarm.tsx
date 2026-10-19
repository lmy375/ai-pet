import { useEffect, useMemo, useState } from "react";
import { useSwarm, type SwarmTask, type WorkerProfile } from "../../hooks/useSwarm";
import { useSettings } from "../../hooks/useSettings";
import { useI18n } from "../../i18n";
import { ChatThread } from "../ChatThread";
import { ChatInput } from "../ChatInput";
import { Button } from "../ui/Button";
import { Segmented } from "../ui/Segmented";
import { LoadingScreen } from "../ui/feedback";

type Conversation = { kind: "main" } | { kind: "worker"; id: string };

function statusLabel(status: string, t: (key: string) => string) {
  if (status === "pending") return t("swarm.task.pending");
  if (status === "running") return t("swarm.task.running");
  return t("swarm.task.completed");
}

function TaskCard({ task, t }: { task: SwarmTask; t: (key: string) => string }) {
  return (
    <div className="rounded-card border border-line bg-surface p-3 shadow-sm">
      <div className="flex items-start justify-between gap-2">
        <div className="min-w-0">
          <div className="truncate text-body font-medium text-ink">{task.title}</div>
          <div className="mt-1 line-clamp-2 text-note text-ink-soft">{task.detail}</div>
        </div>
        <span className="shrink-0 rounded-full bg-surface-soft px-2 py-0.5 text-meta text-ink-soft">{statusLabel(task.status, t)}</span>
      </div>
      <div className="mt-3 h-1.5 overflow-hidden rounded-full bg-line">
        <div className="h-full rounded-full bg-accent transition-all" style={{ width: `${task.progress}%` }} />
      </div>
      <div className="mt-1 flex justify-between text-meta text-ink-faint">
        <span>{task.progress}% {task.progress_summary}</span>
        <span>{task.attempt > 0 ? `${t("swarm.task.attempt")} ${task.attempt}` : ""}</span>
      </div>
      {task.result && <div className="mt-2 max-h-24 overflow-y-auto rounded-field bg-surface-soft px-2 py-1.5 text-note text-ink-soft">{task.result}</div>}
    </div>
  );
}

export function PanelSwarm() {
  const { t } = useI18n();
  const { settings, loaded } = useSettings();
  const swarm = useSwarm();
  const [conversation, setConversation] = useState<Conversation>({ kind: "main" });
  const [pool, setPool] = useState<WorkerProfile[]>([]);
  const [mainAgentId, setMainAgentId] = useState("");
  const [taskTitle, setTaskTitle] = useState("");
  const [taskDetail, setTaskDetail] = useState("");
  const [showConfig, setShowConfig] = useState(false);

  useEffect(() => {
    if (!swarm.snapshot) return;
    setMainAgentId(swarm.snapshot.main_agent_id);
    setPool(swarm.snapshot.pool);
    setConversation({ kind: "main" });
  }, [swarm.snapshot?.id]);

  useEffect(() => {
    if (!mainAgentId && settings.agents[0]) setMainAgentId(settings.agents[0].id);
  }, [mainAgentId, settings.agents]);

  const workerName = (id: string) => settings.agents.find((a) => a.id === id)?.name ?? id;
  const tx = (key: string) => t(key as Parameters<typeof t>[0]);
  const workers = swarm.snapshot?.workers ?? [];
  const activeView = conversation.kind === "main" ? swarm.mainView : swarm.workerViews[conversation.id];
  const totalWorkers = pool.reduce((sum, item) => sum + item.count, 0);
  const groups = useMemo(() => {
    const tasks = swarm.snapshot?.tasks ?? [];
    return {
      pending: tasks.filter((task) => task.status === "pending"),
      running: tasks.filter((task) => task.status === "running"),
      completed: tasks.filter((task) => task.status === "completed"),
    };
  }, [swarm.snapshot?.tasks]);

  if (!loaded) return <LoadingScreen />;
  if (!swarm.snapshot) {
    return (
      <div className="flex h-full items-center justify-center bg-canvas">
        <Button onClick={() => swarm.create(t("swarm.defaultTitle"), settings.active_agent)}>{t("swarm.create")}</Button>
      </div>
    );
  }

  const saveConfig = () => swarm.configure(mainAgentId, pool.filter((item) => item.count > 0));
  const addTask = async () => {
    if (!taskTitle.trim() || !taskDetail.trim()) return;
    await swarm.createTask(taskTitle, taskDetail);
    setTaskTitle("");
    setTaskDetail("");
  };

  const conversationOptions = [
    { value: "main", label: `${t("swarm.mainAgent")} · ${workerName(swarm.snapshot.main_agent_id)}` },
    ...workers.map((worker) => ({ value: worker.id, label: `${workerName(worker.agent_id)} · ${worker.task_id?.slice(0, 8) ?? ""}` })),
  ];

  return (
    <div className="flex h-full min-h-0 w-full bg-canvas">
      <aside className="flex w-[360px] shrink-0 flex-col border-r border-line bg-surface">
        <div className="flex shrink-0 items-center gap-2 border-b border-line px-3 py-2">
          <select value={swarm.activeId} onChange={(e) => swarm.setActiveId(e.target.value)} className="min-w-0 flex-1 rounded-field border border-line bg-surface-soft px-2 py-1.5 text-body text-ink">
            {swarm.swarms.map((item) => <option key={item.id} value={item.id}>{item.title}</option>)}
          </select>
          <Button size="sm" onClick={() => swarm.create(t("swarm.defaultTitle"), settings.active_agent)}>{t("swarm.new")}</Button>
        </div>
        <div className="flex shrink-0 flex-wrap items-center gap-1.5 border-b border-line px-3 py-2">
          <span className="rounded-full bg-accent-soft px-2 py-1 text-meta text-accent">{t(`swarm.status.${swarm.snapshot.status}`)}</span>
          {swarm.snapshot.status === "running" && <Button size="sm" variant="ghost" onClick={() => swarm.end()}>{t("swarm.end")}</Button>}
          {swarm.snapshot.status === "draining" && <Button size="sm" variant="ghost" onClick={() => swarm.stop()}>{t("swarm.stop")}</Button>}
          {swarm.snapshot.status !== "running" && swarm.snapshot.status !== "draining" && <Button size="sm" onClick={() => swarm.start()}>{t("swarm.start")}</Button>}
          {(swarm.snapshot.status === "running" || swarm.snapshot.status === "draining") && <Button size="sm" variant="danger" onClick={() => swarm.stop()}>{t("swarm.stop")}</Button>}
          <Button size="sm" variant="ghost" onClick={() => setShowConfig((value) => !value)}>{t("swarm.configure")}</Button>
        </div>

        {showConfig && (
          <div className="shrink-0 border-b border-line bg-surface-soft px-3 py-2">
            <label className="mb-1 block text-meta text-ink-faint">{t("swarm.mainAgent")}</label>
            <select value={mainAgentId} onChange={(e) => setMainAgentId(e.target.value)} className="mb-2 w-full rounded-field border border-line bg-surface px-2 py-1.5 text-note text-ink">
              {settings.agents.map((agent) => <option key={agent.id} value={agent.id}>{agent.name}</option>)}
            </select>
            <div className="mb-1 flex items-center justify-between text-meta text-ink-faint"><span>{t("swarm.pool")}</span><span>{totalWorkers}</span></div>
            {settings.agents.map((agent) => {
              const current = pool.find((item) => item.agent_id === agent.id);
              return <div key={agent.id} className="mb-2 rounded-field border border-line bg-surface p-2"><div className="flex items-center gap-2"><span className="min-w-0 flex-1 truncate text-note text-ink">{agent.name}</span><input type="number" min={0} max={64} value={current?.count ?? 0} onChange={(e) => setPool((prev) => [...prev.filter((item) => item.agent_id !== agent.id), { agent_id: agent.id, count: Number(e.target.value) || 0, role_prompt: current?.role_prompt ?? "" }])} className="w-14 rounded-field border border-line bg-surface-soft px-2 py-1 text-note text-ink" /></div><textarea value={current?.role_prompt ?? ""} onChange={(e) => setPool((prev) => [...prev.filter((item) => item.agent_id !== agent.id), { agent_id: agent.id, count: current?.count ?? 0, role_prompt: e.target.value }])} placeholder={t("swarm.rolePrompt")} rows={2} className="mt-1 w-full resize-none bg-transparent text-meta text-ink outline-none" /></div>;
            })}
            <Button size="sm" onClick={saveConfig}>{t("common.save")}</Button>
          </div>
        )}

        <div className="min-h-0 flex-1 overflow-y-auto p-3">
          <div className="mb-2 flex items-center justify-between text-note font-medium text-ink"><span>{t("swarm.tasks")}</span><span className="text-ink-faint">{swarm.snapshot.tasks.length}</span></div>
          <div className="mb-3 rounded-card border border-line bg-surface-soft p-2">
            <input value={taskTitle} onChange={(e) => setTaskTitle(e.target.value)} placeholder={t("swarm.task.title")} className="mb-1 w-full bg-transparent text-note text-ink outline-none" />
            <textarea value={taskDetail} onChange={(e) => setTaskDetail(e.target.value)} placeholder={t("swarm.task.detail")} rows={2} className="w-full resize-none bg-transparent text-note text-ink outline-none" />
            <Button size="sm" onClick={addTask}>{t("swarm.task.add")}</Button>
          </div>
          {(["pending", "running", "completed"] as const).map((status) => <div key={status} className="mb-4"><div className="mb-1 text-meta font-medium text-ink-faint">{statusLabel(status, tx)} · {groups[status].length}</div><div className="flex flex-col gap-2">{groups[status].map((task) => <TaskCard key={task.id} task={task} t={tx} />)}</div></div>)}
        </div>
      </aside>

      <section className="flex min-w-0 flex-1 flex-col">
        <div className="flex shrink-0 items-center gap-2 border-b border-line bg-surface px-4 py-2.5">
          <div className="min-w-0 flex-1 overflow-x-auto"><Segmented value={conversation.kind === "main" ? "main" : conversation.id} options={conversationOptions} onChange={(value) => setConversation(value === "main" ? { kind: "main" } : { kind: "worker", id: value })} /></div>
          {conversation.kind === "main" && <span className="text-meta text-ink-faint">{totalWorkers} {t("swarm.workers")}</span>}
        </div>
        <ChatThread items={activeView.items} currentToolCalls={activeView.toolCalls} streaming={activeView.streaming} streamingReasoning={activeView.reasoning} loading={activeView.running || (conversation.kind === "main" && swarm.mainRunning)} className="min-h-0 flex-1" emptyHint={conversation.kind === "main" ? t("swarm.mainEmpty") : t("swarm.workerEmpty")} assistantName={conversation.kind === "main" ? workerName(swarm.snapshot.main_agent_id) : workerName(workers.find((worker) => worker.id === conversation.id)?.agent_id ?? "")}/>
        {conversation.kind === "main" && <div className="shrink-0 border-t border-line bg-surface px-4 py-3"><ChatInput onSend={swarm.sendMain} isLoading={swarm.mainRunning} onStop={swarm.cancelMain} placeholder={t("swarm.mainPlaceholder")} /></div>}
      </section>
    </div>
  );
}
