import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useEvent } from "../lib/events";
import type { ChatItem } from "./useChat";
import { applyStream, emptyView, type StreamEvent, type StreamView } from "./streamView";

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
  main: { items: ChatItem[] };
  workers: SwarmWorkerRecord[];
  created_at: string;
  updated_at: string;
}

/** Same shape and fold as the chat window — see `streamView.ts`. */
export type AgentView = StreamView;

interface WorkerStreamPayload { swarmId: string; workerId: string; event: StreamEvent }
interface MainStreamPayload { swarmId: string; event: StreamEvent }
interface WorkerStatusPayload { swarmId: string; workerId: string; status: string }


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

  useEffect(() => { loadList().catch(console.error); }, [loadList]);
  useEffect(() => { if (activeId) load(activeId).catch(console.error); }, [activeId, load]);

  useEvent<SwarmSnapshot>("swarm-state", applySnapshot);
  useEvent<MainStreamPayload>("swarm-main-stream", ({ swarmId, event }) => {
    if (swarmId !== activeRef.current) return;
    setMainRunning(true);
    setMainView((v) => applyStream(v, event));
  });
  useEvent<{ swarmId: string }>("swarm-main-done", ({ swarmId }) => {
    if (swarmId !== activeRef.current) return;
    setMainRunning(false);
    load(swarmId).catch(console.error);
  });
  useEvent<WorkerStreamPayload>("swarm-worker-stream", ({ swarmId, workerId, event }) => {
    if (swarmId !== activeRef.current) return;
    setWorkerViews((prev) => ({ ...prev, [workerId]: applyStream(prev[workerId] ?? emptyView(), event) }));
  });
  useEvent<WorkerStatusPayload>("swarm-worker-status", ({ swarmId, workerId, status }) => {
    if (swarmId !== activeRef.current) return;
    setWorkerViews((prev) => ({ ...prev, [workerId]: { ...(prev[workerId] ?? emptyView()), running: status === "running" } }));
  });

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
  return { swarms, activeId, setActiveId, snapshot, mainView, workerViews, mainRunning, create, configure, createTask, deleteTask, deleteSwarm, start, end, stop, sendMain, cancelMain };
}
