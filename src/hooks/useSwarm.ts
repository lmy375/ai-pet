import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { newItemId, type ChatItem, type ToolCall } from "./useChat";

export type SwarmStatus = "idle" | "running" | "draining" | "stopped" | "completed";
export type SwarmTaskStatus = "pending" | "running" | "completed";

export interface WorkerProfile {
  agent_id: string;
  count: number;
}

export interface SwarmConfig {
  title: string;
  main_agent_id: string;
  pool: WorkerProfile[];
  shared_prompt: string;
  orchestrator_prompt: string;
  worker_prompt: string;
}

export interface SwarmTask {
  id: string;
  title: string;
  detail: string;
  status: SwarmTaskStatus;
  progress: number;
  progress_summary: string;
  claimed_by?: string;
  parent_task_id?: string;
  result?: string;
  outcome?: string;
  attempt: number;
  created_at: string;
  updated_at: string;
}

export interface SwarmWorkerRecord {
  id: string;
  agent_id: string;
  task_id?: string;
  status: string;
  items: ChatItem[];
}

export interface SwarmSnapshot {
  id: string;
  title: string;
  main_agent_id: string;
  pool: WorkerProfile[];
  shared_prompt: string;
  orchestrator_prompt: string;
  worker_prompt: string;
  status: SwarmStatus;
  tasks: SwarmTask[];
  main: { messages: unknown[]; items: ChatItem[] };
  workers: SwarmWorkerRecord[];
  created_at: string;
  updated_at: string;
}

export interface AgentView {
  items: ChatItem[];
  toolCalls: ToolCall[];
  streaming: string;
  reasoning: string;
  running: boolean;
}

type StreamEvent =
  | { event: "chunk"; data: { text: string } }
  | { event: "reasoning"; data: { text: string } }
  | { event: "toolStart"; data: { name: string; arguments: string } }
  | { event: "toolResult"; data: { name: string; result: string } }
  | { event: "image"; data: { dataUrl: string } }
  | { event: "usage"; data: Record<string, number> }
  | { event: "done"; data: Record<string, never> }
  | { event: "error"; data: { message: string } };

function emptyView(items: ChatItem[] = []): AgentView {
  return { items, toolCalls: [], streaming: "", reasoning: "", running: false };
}

function applyStream(view: AgentView, event: StreamEvent): AgentView {
  const flushTools = (v: AgentView): AgentView => v.toolCalls.length === 0 ? v : {
    ...v,
    items: [...v.items, { id: newItemId(), type: "tool", content: "", toolCalls: v.toolCalls, ts: Date.now() }],
    toolCalls: [],
  };
  const commitText = (v: AgentView): AgentView => !v.streaming.trim() && !v.reasoning.trim() ? v : {
    ...v,
    items: [...v.items, { id: newItemId(), type: "assistant", content: v.streaming, reasoning: v.reasoning || undefined, ts: Date.now() }],
    streaming: "",
    reasoning: "",
  };
  switch (event.event) {
    case "chunk": { const v = flushTools(view); return { ...v, streaming: v.streaming + event.data.text, running: true }; }
    case "reasoning": return { ...view, reasoning: view.reasoning + event.data.text, running: true };
    case "toolStart": { const v = commitText(view); return { ...v, toolCalls: [...v.toolCalls, { name: event.data.name, arguments: event.data.arguments, isRunning: true }], running: true }; }
    case "toolResult": {
      let matched = false;
      return { ...view, toolCalls: view.toolCalls.map((tc) => {
        if (!matched && tc.name === event.data.name && tc.isRunning) { matched = true; return { ...tc, result: event.data.result, isRunning: false }; }
        return tc;
      }) };
    }
    case "image": { const v = flushTools(view); return { ...v, items: [...v.items, { id: newItemId(), type: "assistant", content: "", images: [event.data.dataUrl], ts: Date.now() }] }; }
    case "done": return { ...commitText(flushTools(view)), running: false };
    case "error": return { ...flushTools(view), items: [...flushTools(view).items, { id: newItemId(), type: "error", content: event.data.message, ts: Date.now() }], streaming: "", reasoning: "", running: false };
    default: return view;
  }
}

interface WorkerStreamPayload { swarmId: string; workerId: string; event: StreamEvent }
interface MainStreamPayload { swarmId: string; event: StreamEvent }
interface WorkerStatusPayload { swarmId: string; workerId: string; status: string }

interface Handlers {
  state: (snapshot: SwarmSnapshot) => void;
  mainStream: (payload: MainStreamPayload) => void;
  mainDone: (payload: { swarmId: string }) => void;
  workerStream: (payload: WorkerStreamPayload) => void;
  workerStatus: (payload: WorkerStatusPayload) => void;
}

let handlers: Handlers | null = null;
let listenersStarted = false;
function ensureListeners() {
  if (listenersStarted) return;
  listenersStarted = true;
  listen<SwarmSnapshot>("swarm-state", (e) => handlers?.state(e.payload));
  listen<MainStreamPayload>("swarm-main-stream", (e) => handlers?.mainStream(e.payload));
  listen<{ swarmId: string }>("swarm-main-done", (e) => handlers?.mainDone(e.payload));
  listen<WorkerStreamPayload>("swarm-worker-stream", (e) => handlers?.workerStream(e.payload));
  listen<WorkerStatusPayload>("swarm-worker-status", (e) => handlers?.workerStatus(e.payload));
}

export function useSwarm() {
  const [swarms, setSwarms] = useState<SwarmSnapshot[]>([]);
  const [activeId, setActiveId] = useState("");
  const [snapshot, setSnapshot] = useState<SwarmSnapshot | null>(null);
  const [mainView, setMainView] = useState<AgentView>(emptyView());
  const [workerViews, setWorkerViews] = useState<Record<string, AgentView>>({});
  const [mainRunning, setMainRunning] = useState(false);
  const activeRef = useRef(activeId);
  activeRef.current = activeId;

  const applySnapshot = useCallback((next: SwarmSnapshot) => {
    if (next.id !== activeRef.current) return;
    setSnapshot(next);
    setMainView((prev) => ({ ...prev, items: next.main.items }));
    setWorkerViews((prev) => {
      const views: Record<string, AgentView> = {};
      for (const worker of next.workers) {
        const current = prev[worker.id] ?? emptyView();
        views[worker.id] = { ...current, items: worker.items.length > 0 ? worker.items : current.items, running: worker.status === "running" };
      }
      return views;
    });
  }, []);

  const loadList = useCallback(async () => {
    const list = await invoke<SwarmSnapshot[]>("swarm_list");
    setSwarms(list);
    if (!activeRef.current && list[0]) setActiveId(list[0].id);
  }, []);

  const load = useCallback(async (id: string) => {
    const next = await invoke<SwarmSnapshot>("swarm_load", { id });
    setSnapshot(next);
    setMainView(emptyView(next.main.items));
    const views: Record<string, AgentView> = {};
    for (const worker of next.workers) views[worker.id] = emptyView(worker.items);
    setWorkerViews(views);
  }, []);

  useEffect(() => { ensureListeners(); loadList().catch(console.error); }, [loadList]);
  useEffect(() => { if (activeId) load(activeId).catch(console.error); }, [activeId, load]);
  useEffect(() => {
    const next: Handlers = {
      state: applySnapshot,
      mainStream: ({ swarmId, event }) => { if (swarmId !== activeRef.current) return; setMainRunning(true); setMainView((v) => applyStream(v, event)); },
      mainDone: ({ swarmId }) => { if (swarmId !== activeRef.current) return; setMainRunning(false); load(swarmId).catch(console.error); },
      workerStream: ({ swarmId, workerId, event }) => { if (swarmId !== activeRef.current) return; setWorkerViews((prev) => ({ ...prev, [workerId]: applyStream(prev[workerId] ?? emptyView(), event) })); },
      workerStatus: ({ swarmId, workerId, status }) => { if (swarmId !== activeRef.current) return; setWorkerViews((prev) => ({ ...prev, [workerId]: { ...(prev[workerId] ?? emptyView()), running: status === "running" } })); },
    };
    handlers = next;
    return () => { if (handlers === next) handlers = null; };
  }, [applySnapshot, load]);

  const create = useCallback(async (title: string, mainAgentId: string) => {
    const created = await invoke<SwarmSnapshot>("swarm_create", { title, mainAgentId });
    setSwarms((prev) => [created, ...prev]);
    setActiveId(created.id);
  }, []);
  const configure = useCallback(async (config: SwarmConfig): Promise<void> => {
    if (!activeId) return;
    await invoke("swarm_configure", { id: activeId, config });
    await load(activeId);
    await loadList();
  }, [activeId, load, loadList]);
  const createTask = useCallback(async (title: string, detail: string, parentTaskId?: string): Promise<void> => {
    if (!activeId) return;
    await invoke("swarm_create_task", { id: activeId, title, detail, parentTaskId });
    await load(activeId);
  }, [activeId, load]);
  const deleteTask = useCallback(async (taskId: string): Promise<void> => {
    if (!activeId) return;
    await invoke("swarm_delete_task", { id: activeId, taskId });
    await load(activeId);
  }, [activeId, load]);
  const deleteSwarm = useCallback(async (): Promise<void> => {
    const id = activeRef.current;
    if (!id) return;
    await invoke("swarm_delete", { id });
    const list = await invoke<SwarmSnapshot[]>("swarm_list");
    setSwarms(list);
    const next = list[0] ?? null;
    setSnapshot(next);
    setMainView(emptyView(next?.main.items ?? []));
    setWorkerViews({});
    setActiveId(next?.id ?? "");
  }, []);
  const start = useCallback(async (): Promise<void> => {
    if (activeId) await invoke("swarm_start", { id: activeId });
  }, [activeId]);
  const end = useCallback(async (): Promise<void> => {
    if (activeId) await invoke("swarm_end", { id: activeId });
  }, [activeId]);
  const stop = useCallback(async (): Promise<void> => {
    if (activeId) await invoke("swarm_stop", { id: activeId });
  }, [activeId]);
  const sendMain = useCallback(async (text: string, images?: string[]): Promise<void> => {
    if (activeId) await invoke("swarm_send_main", { id: activeId, turn: { text, images: images ?? [] } });
  }, [activeId]);
  const cancelMain = useCallback(async (): Promise<void> => {
    if (activeId) await invoke("swarm_cancel_main", { id: activeId });
  }, [activeId]);
  const reload = useCallback(async (): Promise<void> => {
    if (activeId) await load(activeId);
  }, [activeId, load]);

  return { swarms, activeId, setActiveId, snapshot, mainView, workerViews, mainRunning, create, configure, createTask, deleteTask, deleteSwarm, start, end, stop, sendMain, cancelMain, reload };
}
