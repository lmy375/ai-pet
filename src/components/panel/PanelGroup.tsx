import { useEffect, useRef, useState } from "react";
import { useGroupChat } from "../../hooks/useGroupChat";
import type { AgentView, GroupMessage } from "../../hooks/useGroupChat";
import { useSettings } from "../../hooks/useSettings";
import { useI18n } from "../../i18n";
import { ChatThread } from "../ChatThread";
import { ChatInput } from "../ChatInput";
import { MessageBubble } from "../ui/MessageBubble";
import { Segmented } from "../ui/Segmented";
import { IconActionButton } from "../ui/IconButton";
import { AgentIcon, TrashIcon, CheckIcon, SpinnerIcon, PauseIcon, PlayIcon } from "../Icons";

// Stable per-agent accent (by membership position) for the transcript labels.
const AGENT_DOTS = [
  "bg-violet-400",
  "bg-emerald-400",
  "bg-amber-400",
  "bg-sky-400",
  "bg-rose-400",
  "bg-teal-400",
];

/** The shared transcript view: human bubbles right, agent bubbles left with a
 *  name label + per-agent color dot. */
function GroupTranscript({
  transcript,
  colorFor,
  emptyHint,
}: {
  transcript: GroupMessage[];
  colorFor: (agentId?: string) => string;
  emptyHint: string;
}) {
  const endRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    endRef.current?.scrollIntoView({ block: "end" });
  }, [transcript]);

  if (transcript.length === 0) {
    return <div className="mt-10 text-center text-[14px] text-slate-400">{emptyHint}</div>;
  }

  return (
    <div className="flex flex-col gap-2.5">
      {transcript.map((m) =>
        m.speaker_kind === "human" ? (
          <MessageBubble key={m.id} role="user">
            {m.content}
          </MessageBubble>
        ) : (
          <div key={m.id} className="flex flex-col items-start gap-0.5">
            <div className="flex items-center gap-1.5 px-1 text-[11px] font-medium text-slate-500">
              <span className={`h-2 w-2 rounded-full ${colorFor(m.agent_id)}`} />
              {m.name}
            </div>
            <MessageBubble role="assistant">{m.content}</MessageBubble>
          </div>
        ),
      )}
      <div ref={endRef} />
    </div>
  );
}

/** Popover to pick which configured agents join the group. */
function MemberPicker({
  allAgents,
  members,
  onToggle,
}: {
  allAgents: { id: string; name: string }[];
  members: string[];
  onToggle: (id: string) => void;
}) {
  const { t } = useI18n();
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [open]);

  return (
    <div ref={ref} className="relative">
      <button
        onClick={() => setOpen((o) => !o)}
        title={t("group.members")}
        className="flex h-8 items-center gap-1.5 rounded-lg border border-slate-200 bg-white px-2.5 text-[12px] font-medium text-slate-700 transition-colors hover:border-slate-300"
      >
        <AgentIcon className="h-4 w-4" />
        {t("group.members")} ({members.length})
      </button>
      {open && (
        <div className="absolute right-0 z-10 mt-1 w-52 rounded-lg border border-slate-200 bg-white p-1 shadow-lg">
          {allAgents.length === 0 && (
            <div className="px-2 py-1.5 text-[12px] text-slate-400">{t("group.noAgents")}</div>
          )}
          {allAgents.map((a) => {
            const checked = members.includes(a.id);
            return (
              <button
                key={a.id}
                onClick={() => onToggle(a.id)}
                className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-[13px] text-slate-700 transition-colors hover:bg-slate-50"
              >
                <span
                  className={`flex h-4 w-4 shrink-0 items-center justify-center rounded border ${
                    checked ? "border-accent bg-accent text-white" : "border-slate-300 bg-white"
                  }`}
                >
                  {checked && <CheckIcon className="h-3 w-3" />}
                </span>
                <span className="truncate">{a.name}</span>
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}

/** A row of currently-thinking agents (their loops are running). */
function ThinkingBar({
  thinking,
  colorFor,
}: {
  thinking: { id: string; name: string }[];
  colorFor: (agentId?: string) => string;
}) {
  const { t } = useI18n();
  if (thinking.length === 0) return null;
  return (
    <div className="flex flex-wrap items-center gap-x-3 gap-y-1 px-1 py-1 text-[12px] text-slate-500">
      <SpinnerIcon className="h-3.5 w-3.5 animate-spin text-slate-400" />
      {thinking.map((a) => (
        <span key={a.id} className="flex items-center gap-1.5">
          <span className={`h-2 w-2 rounded-full ${colorFor(a.id)}`} />
          {t("group.thinking", { name: a.name })}
        </span>
      ))}
    </div>
  );
}

export function PanelGroup() {
  const { t } = useI18n();
  const { settings, loaded } = useSettings();
  const { transcript, members, agents, paused, anyRunning, send, updateMembers, reset, setPaused } = useGroupChat();

  // Active sub-tab: "group" or a member agent id.
  const [subTab, setSubTab] = useState<string>("group");

  // If the active agent tab is removed from the group, fall back to the room.
  useEffect(() => {
    if (subTab !== "group" && !members.includes(subTab)) setSubTab("group");
  }, [members, subTab]);

  const nameOf = (id: string) => settings.agents.find((a) => a.id === id)?.name ?? id;
  const colorFor = (agentId?: string) => {
    const i = agentId ? members.indexOf(agentId) : -1;
    return i >= 0 ? AGENT_DOTS[i % AGENT_DOTS.length] : "bg-slate-400";
  };

  const toggleMember = (id: string) => {
    const next = members.includes(id) ? members.filter((m) => m !== id) : [...members, id];
    updateMembers(next);
  };

  const tabs = [
    { value: "group" as const, label: t("group.tab.room") },
    ...members.map((id) => ({ value: id, label: nameOf(id) })),
  ];

  const thinking = members.filter((id) => agents[id]?.running).map((id) => ({ id, name: nameOf(id) }));

  if (!loaded) return null;

  return (
    <div className="flex h-full w-full flex-col bg-slate-100">
      {/* Header: sub-tabs + pause + member picker + reset */}
      <div className="flex shrink-0 items-center justify-between gap-2 border-b border-slate-200/70 bg-white/70 px-3 py-2">
        <div className="min-w-0 overflow-x-auto">
          <Segmented value={subTab} options={tabs} onChange={setSubTab} />
        </div>
        <div className="flex shrink-0 items-center gap-1.5">
          <button
            onClick={() => setPaused(!paused)}
            title={paused ? t("group.resume") : t("group.pause")}
            className={`flex h-8 items-center gap-1.5 rounded-lg border px-2.5 text-[12px] font-medium transition-colors ${
              paused
                ? "border-accent/60 bg-accent/10 text-accent hover:bg-accent/15"
                : "border-slate-200 bg-white text-slate-700 hover:border-slate-300"
            }`}
          >
            {paused ? <PlayIcon className="h-4 w-4" /> : <PauseIcon className="h-4 w-4" />}
            {paused ? t("group.resume") : t("group.pause")}
          </button>
          <MemberPicker
            allAgents={settings.agents.map((a) => ({ id: a.id, name: a.name }))}
            members={members}
            onToggle={toggleMember}
          />
          <IconActionButton variant="danger" onClick={reset} title={t("group.reset")}>
            <TrashIcon className="h-4 w-4" />
          </IconActionButton>
        </div>
      </div>

      {/* Body */}
      {subTab === "group" ? (
        <>
          <div className="flex-1 overflow-y-auto px-3 py-3">
            <GroupTranscript
              transcript={transcript}
              colorFor={colorFor}
              emptyHint={members.length === 0 ? t("group.empty.noMembers") : t("group.empty.room")}
            />
          </div>
          <div className="shrink-0 border-t border-slate-200/70 bg-white/70 px-3 py-2">
            {paused && anyRunning === false && transcript.length > 0 && (
              <div className="px-1 pb-1 text-[12px] text-slate-400">{t("group.pausedNote")}</div>
            )}
            <ThinkingBar thinking={thinking} colorFor={colorFor} />
            <ChatInput onSend={send} isLoading={false} placeholder={t("group.placeholder")} />
          </div>
        </>
      ) : (
        <div className="flex-1 overflow-hidden px-3 py-3">
          <AgentTab view={agents[subTab]} emptyHint={t("group.agent.empty")} />
        </div>
      )}
    </div>
  );
}

/** One agent's private session (full thinking + tool calls), reusing ChatThread. */
function AgentTab({ view, emptyHint }: { view: AgentView | undefined; emptyHint: string }) {
  if (!view) return <div className="mt-10 text-center text-[14px] text-slate-400">{emptyHint}</div>;
  return (
    <ChatThread
      items={view.items}
      currentToolCalls={view.toolCalls}
      streaming={view.streaming}
      loading={view.running}
      className="h-full"
      emptyHint={emptyHint}
    />
  );
}
