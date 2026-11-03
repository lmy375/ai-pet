import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useEvent } from "../lib/events";
import type { ChatItem } from "./useChat";
import { applyStream, emptyView, type StreamEvent, type StreamView } from "./streamView";

/** One line in the shared group transcript (mirrors backend `GroupMessage`). */
export interface GroupMessage {
  id: string;
  speaker_kind: "human" | "agent";
  agent_id?: string;
  name: string;
  content: string;
  ts: number;
}

/** Room record for the sidebar (`group_list`). */
export interface GroupSummary {
  id: string;
  title: string;
  topic: string;
  members: string[];
  paused: boolean;
  updated_at: number;
}

/** Live + committed display state for one agent's session (Agent sub-tab).
 *  Same shape and fold as the chat window — see `streamView.ts`. */
export type AgentView = StreamView;

/** Backend `GroupState` snapshot from `group_load`. */
interface GroupStateSnapshot {
  id: string;
  title: string;
  topic: string;
  transcript: GroupMessage[];
  members: string[];
  agents: Record<string, { items?: ChatItem[] }>;
  paused: boolean;
}


/** Drives the group-chat page: the room list, the active room's shared
 *  transcript, per-agent live sessions, membership and pause state. */
export function useGroupChat() {
  const [groups, setGroups] = useState<GroupSummary[]>([]);
  const [ready, setReady] = useState(false);
  const [activeId, setActiveId] = useState("");
  const [title, setTitle] = useState("");
  const [topic, setTopic] = useState("");
  const [transcript, setTranscript] = useState<GroupMessage[]>([]);
  const [members, setMembers] = useState<string[]>([]);
  const [agents, setAgents] = useState<Record<string, AgentView>>({});
  const [paused, setPausedState] = useState(false);
  // Dedup transcript appends — group_load + the group-message event for the same
  // post can race on first mount.
  const seenMsgIds = useRef<Set<string>>(new Set());
  const activeRef = useRef(activeId);
  activeRef.current = activeId;

  const load = useCallback(async (id: string) => {
    const state = await invoke<GroupStateSnapshot>("group_load", { id });
    // The user may have switched rooms while this load was in flight.
    if (activeRef.current !== id) return;
    seenMsgIds.current = new Set(state.transcript.map((m) => m.id));
    setTitle(state.title);
    setTopic(state.topic ?? "");
    setTranscript(state.transcript);
    setMembers(state.members);
    const views: Record<string, AgentView> = {};
    for (const [agentId, st] of Object.entries(state.agents)) {
      views[agentId] = emptyView(st.items ?? []);
    }
    setAgents(views);
    setPausedState(state.paused);
  }, []);

  const loadList = useCallback(async (selectId?: string) => {
    const list = await invoke<GroupSummary[]>("group_list");
    setGroups(list);
    setActiveId((prev) => {
      const wanted = selectId ?? prev;
      if (wanted && list.some((group) => group.id === wanted)) return wanted;
      return list[0]?.id ?? "";
    });
  }, []);

  // Stable handlers (functional setState + refs only) so the module singleton can
  // hold them across renders without re-subscribing.
  const onMessage = useCallback(({ groupId, message }: { groupId: string; message: GroupMessage }) => {
    if (groupId !== activeRef.current || seenMsgIds.current.has(message.id)) return;
    seenMsgIds.current.add(message.id);
    setTranscript((prev) => [...prev, message]);
  }, []);

  const onInjected = useCallback(({ groupId, agentId, items }: { groupId: string; agentId: string; items: ChatItem[] }) => {
    if (groupId !== activeRef.current) return;
    setAgents((prev) => {
      const view = prev[agentId] ?? emptyView();
      // `group_load` may already carry items that were injected just before the
      // switch; only append the ones this view hasn't rendered yet.
      const seen = new Set(view.items.map((item) => item.id));
      const fresh = items.filter((item) => !seen.has(item.id));
      return { ...prev, [agentId]: { ...view, items: [...view.items, ...fresh], running: true } };
    });
  }, []);

  const onStream = useCallback(({ groupId, agentId, event }: { groupId: string; agentId: string; event: StreamEvent }) => {
    if (groupId !== activeRef.current) return;
    setAgents((prev) => {
      const view = prev[agentId] ?? emptyView();
      return { ...prev, [agentId]: applyStream(view, event) };
    });
  }, []);

  const onAgentDone = useCallback(({ groupId, agentId }: { groupId: string; agentId: string }) => {
    if (groupId !== activeRef.current) return;
    setAgents((prev) => {
      const view = prev[agentId];
      if (!view || !view.running) return prev;
      return { ...prev, [agentId]: { ...view, running: false } };
    });
  }, []);

  // Pausing stops all loops server-side; clear every agent's running indicator.
  const onPaused = useCallback(({ groupId }: { groupId: string }) => {
    setGroups((prev) => prev.map((group) => (group.id === groupId ? { ...group, paused: true } : group)));
    if (groupId !== activeRef.current) return;
    setPausedState(true);
    setAgents((prev) => {
      const next: Record<string, AgentView> = {};
      for (const [id, v] of Object.entries(prev)) next[id] = { ...v, running: false };
      return next;
    });
  }, []);

  const onResumed = useCallback(({ groupId }: { groupId: string }) => {
    setGroups((prev) => prev.map((group) => (group.id === groupId ? { ...group, paused: false } : group)));
    if (groupId === activeRef.current) setPausedState(false);
  }, []);

  const onReset = useCallback(({ groupId }: { groupId: string }) => {
    if (groupId !== activeRef.current) return;
    seenMsgIds.current = new Set();
    setTranscript([]);
    setAgents((prev) => {
      const next: Record<string, AgentView> = {};
      for (const id of Object.keys(prev)) next[id] = emptyView();
      return next;
    });
    setPausedState(false);
  }, []);

  useEffect(() => {
    loadList().finally(() => setReady(true));
  }, [loadList]);

  useEvent<{ groupId: string; message: GroupMessage }>("group-message", onMessage);
  useEvent<{ groupId: string; agentId: string; items: ChatItem[] }>("group-injected", onInjected);
  useEvent<{ groupId: string; agentId: string; event: StreamEvent }>("group-stream", onStream);
  useEvent<{ groupId: string; agentId: string }>("group-agent-done", onAgentDone);
  useEvent<{ groupId: string }>("group-paused", onPaused);
  useEvent<{ groupId: string }>("group-resumed", onResumed);
  useEvent<{ groupId: string }>("group-reset", onReset);

  useEffect(() => {
    if (!activeId) {
      seenMsgIds.current = new Set();
      setTitle("");
      setTopic("");
      setTranscript([]);
      setMembers([]);
      setAgents({});
      setPausedState(false);
      return;
    }
    load(activeId).catch(console.error);
  }, [activeId, load]);

  const create = useCallback(async (title: string, topic: string, members: string[]): Promise<void> => {
    const state = await invoke<GroupStateSnapshot>("group_create", { title, topic, members });
    const summary: GroupSummary = {
      id: state.id,
      title: state.title,
      topic: state.topic,
      members: state.members,
      paused: state.paused,
      updated_at: Date.now(),
    };
    setGroups((prev) => [summary, ...prev.filter((group) => group.id !== state.id)]);
    setActiveId(state.id);
  }, []);

  const configure = useCallback(async (id: string, title: string, topic: string, members: string[]): Promise<void> => {
    const trimmed = title.trim();
    const trimmedTopic = topic.trim();
    await invoke("group_configure", { id, title: trimmed, topic: trimmedTopic, members });
    setGroups((prev) =>
      prev.map((group) =>
        group.id === id
          ? { ...group, title: trimmed, topic: trimmedTopic, members, updated_at: Date.now() }
          : group,
      ),
    );
    if (id !== activeRef.current) return;
    setTitle(trimmed);
    setTopic(trimmedTopic);
    setMembers(members);
    setAgents((prev) => {
      const next: Record<string, AgentView> = {};
      for (const agentId of members) next[agentId] = prev[agentId] ?? emptyView();
      return next;
    });
  }, []);

  const deleteGroup = useCallback(async (id: string): Promise<void> => {
    await invoke("group_delete", { id });
    const list = await invoke<GroupSummary[]>("group_list");
    setGroups(list);
    setActiveId((prev) => (prev === id ? (list[0]?.id ?? "") : prev));
  }, []);

  const send = useCallback(async (content: string) => {
    const id = activeRef.current;
    if (id) await invoke("group_send", { id, content });
  }, []);

  const reset = useCallback(async () => {
    const id = activeRef.current;
    if (id) await invoke("group_reset", { id });
  }, []);

  const setPaused = useCallback(async (next: boolean) => {
    const id = activeRef.current;
    if (!id) return;
    // Optimistic — the group-paused/resumed event reconciles + clears indicators.
    setPausedState(next);
    setGroups((prev) => prev.map((group) => (group.id === id ? { ...group, paused: next } : group)));
    await invoke("group_set_paused", { id, paused: next });
  }, []);

  const anyRunning = Object.values(agents).some((v) => v.running);

  return {
    groups,
    ready,
    activeId,
    setActiveId,
    title,
    topic,
    transcript,
    members,
    agents,
    paused,
    anyRunning,
    create,
    configure,
    deleteGroup,
    send,
    reset,
    setPaused,
  };
}
