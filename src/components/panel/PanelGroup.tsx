import { useEffect, useRef, useState } from "react";
import { useGroupChat, type AgentView, type GroupMessage, type GroupSummary } from "../../hooks/useGroupChat";
import { useSettings, type AgentConfig } from "../../hooks/useSettings";
import { useI18n } from "../../i18n";
import { ChatThread } from "../ChatThread";
import { ChatInput } from "../ChatInput";
import { MessageBubble } from "../ui/MessageBubble";
import { Markdown } from "../ui/Markdown";
import { Segmented } from "../ui/Segmented";
import { Button } from "../ui/Button";
import { IconActionButton } from "../ui/IconButton";
import { ResizeHandle } from "../ui/ResizeHandle";
import { useResizableWidth } from "../../hooks/useResizableWidth";
import {
  AgentIcon,
  CheckIcon,
  CloseIcon,
  GearIcon,
  PauseIcon,
  PlayIcon,
  PlusIcon,
  SpinnerIcon,
  TrashIcon,
} from "../Icons";

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
    return <div className="mt-10 text-center text-[14px] text-ink-faint">{emptyHint}</div>;
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
            <div className="flex items-center gap-1.5 px-1 text-[11px] font-medium text-ink-soft">
              <span className={`h-2 w-2 rounded-full ${colorFor(m.agent_id)}`} />
              {m.name}
            </div>
            <MessageBubble role="assistant">
              <Markdown text={m.content} />
            </MessageBubble>
          </div>
        ),
      )}
      <div ref={endRef} />
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
    <div className="flex flex-wrap items-center gap-x-3 gap-y-1 px-1 py-1 text-[12px] text-ink-soft">
      <SpinnerIcon className="h-3.5 w-3.5 animate-spin text-ink-faint" />
      {thinking.map((a) => (
        <span key={a.id} className="flex items-center gap-1.5">
          <span className={`h-2 w-2 rounded-full ${colorFor(a.id)}`} />
          {t("group.thinking", { name: a.name })}
        </span>
      ))}
    </div>
  );
}

/** One agent's private session (full thinking + tool calls), reusing ChatThread
 *  with the same name/reasoning/meta treatment as the panel chat. */
function AgentTab({
  view,
  agentName,
  colorFor,
  emptyHint,
}: {
  view: AgentView | undefined;
  agentName: string;
  colorFor: (agentId?: string) => string;
  emptyHint: string;
}) {
  if (!view) return <div className="mt-10 text-center text-[14px] text-ink-faint">{emptyHint}</div>;
  return (
    <ChatThread
      items={view.items}
      currentToolCalls={view.toolCalls}
      streaming={view.streaming}
      streamingReasoning={view.reasoning}
      loading={view.running}
      className="h-full"
      emptyHint={emptyHint}
      assistantName={agentName}
      groupColorFor={colorFor}
    />
  );
}

/** Create a room, or edit an existing one's name and members. */
function GroupDialog({
  mode,
  group,
  agents,
  onSubmit,
  onDelete,
  onClose,
}: {
  mode: "create" | "edit";
  group?: GroupSummary;
  agents: AgentConfig[];
  onSubmit: (title: string, topic: string, members: string[]) => Promise<void>;
  onDelete?: () => Promise<void>;
  onClose: () => void;
}) {
  const { t } = useI18n();
  const [title, setTitle] = useState(group?.title ?? "");
  const [topic, setTopic] = useState(group?.topic ?? "");
  const [members, setMembers] = useState<string[]>(group?.members ?? []);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  const submit = async () => {
    if (!title.trim() || busy) return;
    setBusy(true);
    setError("");
    try {
      await onSubmit(title.trim(), topic.trim(), members);
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  };

  const remove = async () => {
    if (!group || !onDelete || busy) return;
    if (!window.confirm(t("group.confirmDelete", { title: group.title }))) return;
    setBusy(true);
    setError("");
    try {
      await onDelete();
    } catch (e) {
      setError(String(e));
      setBusy(false);
    }
  };

  const toggle = (id: string) => {
    setMembers((prev) => (prev.includes(id) ? prev.filter((m) => m !== id) : [...prev, id]));
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/25 px-4">
      <div className="w-full max-w-[480px] rounded-card border border-line bg-surface p-5 shadow-pop">
        <div className="flex items-center justify-between gap-2">
          <div className="text-title font-semibold text-ink">
            {mode === "create" ? t("group.create") : t("group.settings")}
          </div>
          <IconActionButton aria-label={t("common.cancel")} title={t("common.cancel")} onClick={onClose} disabled={busy}>
            <CloseIcon className="h-4 w-4" />
          </IconActionButton>
        </div>
        <div className="mt-1 text-note text-ink-soft">
          {mode === "create" ? t("group.createHint") : t("group.settingsHint")}
        </div>

        <label className="mt-4 block text-note font-medium text-ink">{t("group.name")}</label>
        <input
          autoFocus
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && submit()}
          placeholder={t("group.namePlaceholder")}
          className="mt-1 w-full rounded-field border border-line bg-surface-soft px-3 py-2 text-body text-ink outline-none focus:border-accent"
        />

        <label className="mt-3 block text-note font-medium text-ink">{t("group.topic")}</label>
        <textarea
          value={topic}
          onChange={(e) => setTopic(e.target.value)}
          placeholder={t("group.topicPlaceholder")}
          rows={2}
          className="mt-1 w-full resize-none rounded-field border border-line bg-surface-soft px-3 py-2 text-body text-ink outline-none focus:border-accent"
        />

        <div className="mt-4 flex items-center gap-1.5 text-note font-medium text-ink">
          <AgentIcon className="h-4 w-4 text-ink-soft" />
          {t("group.members")}
        </div>
        <div className="mt-1 max-h-[240px] overflow-y-auto rounded-field border border-line bg-surface-soft p-1">
          {agents.length === 0 && (
            <div className="px-2 py-2 text-note text-ink-faint">{t("group.noAgents")}</div>
          )}
          {agents.map((agent) => {
            const checked = members.includes(agent.id);
            return (
              <button
                key={agent.id}
                type="button"
                onClick={() => toggle(agent.id)}
                className="flex w-full items-center gap-2 rounded-field px-2 py-1.5 text-left text-body text-ink transition-colors hover:bg-hover"
              >
                <span
                  className={`flex h-4 w-4 shrink-0 items-center justify-center rounded border ${
                    checked ? "border-accent bg-accent text-white" : "border-line bg-surface"
                  }`}
                >
                  {checked && <CheckIcon className="h-3 w-3" />}
                </span>
                <span className="truncate">{agent.name}</span>
              </button>
            );
          })}
        </div>

        {error && <div className="mt-3 text-note text-red-500">{error}</div>}

        <div className="mt-5 flex items-center gap-2">
          {mode === "edit" && onDelete && (
            <Button variant="danger" size="sm" onClick={remove} disabled={busy}>
              <TrashIcon className="h-4 w-4" />
              {t("group.delete")}
            </Button>
          )}
          <div className="ml-auto flex gap-2">
            <Button variant="ghost" size="sm" onClick={onClose} disabled={busy}>
              {t("common.cancel")}
            </Button>
            <Button size="sm" onClick={submit} disabled={!title.trim() || busy}>
              {mode === "create" ? t("common.create") : t("common.save")}
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}

export function PanelGroup() {
  const { t } = useI18n();
  const { settings, loaded } = useSettings();
  const group = useGroupChat();

  // Active sub-tab: "group" or a member agent id.
  const [subTab, setSubTab] = useState<string>("group");
  const [dialog, setDialog] = useState<{ mode: "create" } | { mode: "edit"; group: GroupSummary } | null>(null);
  const listPane = useResizableWidth({
    storageKey: "group-list",
    initialWidth: 280,
    minWidth: 220,
    maxWidth: 440,
  });

  // If the active agent tab is removed from the group, fall back to the room.
  useEffect(() => {
    if (subTab !== "group" && !group.members.includes(subTab)) setSubTab("group");
  }, [group.members, subTab]);

  const active = group.groups.find((item) => item.id === group.activeId);
  const title = active?.title ?? group.title;
  const nameOf = (id: string) => settings.agents.find((a) => a.id === id)?.name ?? id;
  const colorFor = (agentId?: string) => {
    const i = agentId ? group.members.indexOf(agentId) : -1;
    return i >= 0 ? AGENT_DOTS[i % AGENT_DOTS.length] : "bg-ink-faint";
  };

  const tabs = [
    { value: "group" as const, label: t("group.tab.room") },
    ...group.members.map((id) => ({ value: id, label: nameOf(id) })),
  ];

  const thinking = group.members
    .filter((id) => group.agents[id]?.running)
    .map((id) => ({ id, name: nameOf(id) }));

  if (!loaded) return null;

  return (
    <div className="flex h-full min-h-0 w-full bg-canvas">
      <aside style={{ width: listPane.width }} className="flex min-h-0 shrink-0 flex-col bg-surface">
        <div className="flex shrink-0 items-center justify-between gap-2 border-b border-line px-3 py-2">
          <div className="truncate text-note font-semibold text-ink">{t("group.list")}</div>
          <Button size="sm" onClick={() => setDialog({ mode: "create" })}>
            <PlusIcon className="h-4 w-4" />
            {t("group.new")}
          </Button>
        </div>

        <div className="min-h-0 flex-1 overflow-y-auto p-2">
          {group.ready && group.groups.length === 0 ? (
            <div className="px-2 py-6 text-center text-note text-ink-faint">{t("group.noGroupsHint")}</div>
          ) : (
            <div className="flex flex-col gap-1">
              {group.groups.map((item) => {
                const selected = item.id === group.activeId;
                return (
                  <div
                    key={item.id}
                    role="button"
                    tabIndex={0}
                    aria-current={selected}
                    onClick={() => {
                      setSubTab("group");
                      group.setActiveId(item.id);
                    }}
                    onKeyDown={(event) => {
                      if (event.target !== event.currentTarget) return;
                      if (event.key === "Enter" || event.key === " ") {
                        event.preventDefault();
                        setSubTab("group");
                        group.setActiveId(item.id);
                      }
                    }}
                    className={`flex cursor-pointer items-center gap-1 rounded-field px-2 py-2 transition-colors focus:outline-none focus:ring-1 focus:ring-accent ${
                      selected ? "bg-accent-soft" : "hover:bg-hover"
                    }`}
                  >
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-1.5">
                        {item.paused && (
                          <span title={t("group.pausedNote")} className="h-1.5 w-1.5 shrink-0 rounded-full bg-amber-400" />
                        )}
                        <div className={`truncate text-body font-medium ${selected ? "text-accent" : "text-ink"}`}>
                          {item.title}
                        </div>
                      </div>
                      <div className="truncate text-meta text-ink-faint">
                        {t("group.memberCount", { count: item.members.length })}
                      </div>
                    </div>
                    <IconActionButton
                      size="sm"
                      aria-label={t("group.settings")}
                      title={t("group.settings")}
                      onClick={(event) => {
                        event.stopPropagation();
                        setDialog({ mode: "edit", group: item });
                      }}
                    >
                      <GearIcon className="h-4 w-4" />
                    </IconActionButton>
                  </div>
                );
              })}
            </div>
          )}
        </div>
      </aside>

      <ResizeHandle
        side="left"
        width={listPane.width}
        minWidth={220}
        maxWidth={440}
        onChange={listPane.setWidth}
        label={t("common.resizePanel")}
      />

      {!group.ready ? (
        <section className="min-w-0 flex-1 bg-canvas" />
      ) : !active ? (
        <section className="flex min-w-0 flex-1 items-center justify-center bg-canvas px-6">
          <div className="max-w-[420px] text-center">
            <div className="text-title font-semibold text-ink">{t("group.noGroups")}</div>
            <div className="mt-2 text-body text-ink-soft">{t("group.noGroupsHint")}</div>
            <Button className="mt-5" onClick={() => setDialog({ mode: "create" })}>
              {t("group.new")}
            </Button>
          </div>
        </section>
      ) : (
        <section className="flex min-w-0 flex-1 flex-col">
          {/* Header: room name, members, pause/resume, reset */}
          <div className="flex shrink-0 flex-col gap-2 border-b border-line bg-surface px-4 py-2">
            <div className="flex items-center gap-2">
              <div className="min-w-0 flex-1">
                <div className="truncate text-title font-semibold text-ink">{title}</div>
                {group.topic.trim() && (
                  <div className="truncate text-meta text-ink-soft">{group.topic}</div>
                )}
                <div className="truncate text-meta text-ink-faint">
                  {group.members.length === 0
                    ? t("group.noMembers")
                    : group.members.map(nameOf).join("、")}
                </div>
              </div>
              <button
                onClick={() => group.setPaused(!group.paused)}
                title={group.paused ? t("group.resume") : t("group.pause")}
                className={`flex h-8 shrink-0 items-center gap-1.5 rounded-lg border px-2.5 text-[12px] font-medium transition-colors ${
                  group.paused
                    ? "border-accent/60 bg-accent/10 text-accent hover:bg-accent/15"
                    : "border-line bg-surface text-ink hover:border-accent-line"
                }`}
              >
                {group.paused ? <PlayIcon className="h-4 w-4" /> : <PauseIcon className="h-4 w-4" />}
                {group.paused ? t("group.resume") : t("group.pause")}
              </button>
              <IconActionButton variant="danger" onClick={group.reset} title={t("group.reset")}>
                <TrashIcon className="h-4 w-4" />
              </IconActionButton>
            </div>
            <div className="min-w-0 overflow-x-auto">
              <Segmented value={subTab} options={tabs} onChange={setSubTab} />
            </div>
          </div>

          {/* Body */}
          {subTab === "group" ? (
            <>
              <div className="flex-1 overflow-y-auto px-3 py-3">
                <GroupTranscript
                  transcript={group.transcript}
                  colorFor={colorFor}
                  emptyHint={group.members.length === 0 ? t("group.empty.noMembers") : t("group.empty.room")}
                />
              </div>
              <div className="shrink-0 border-t border-line/70 bg-surface/70 px-3 py-2">
                {group.paused && group.anyRunning === false && group.transcript.length > 0 && (
                  <div className="px-1 pb-1 text-[12px] text-ink-faint">{t("group.pausedNote")}</div>
                )}
                <ThinkingBar thinking={thinking} colorFor={colorFor} />
                <ChatInput onSend={group.send} isLoading={false} placeholder={t("group.placeholder")} />
              </div>
            </>
          ) : (
            <div className="flex-1 overflow-hidden px-3 py-3">
              <AgentTab
                view={group.agents[subTab]}
                agentName={nameOf(subTab)}
                colorFor={colorFor}
                emptyHint={t("group.agent.empty")}
              />
            </div>
          )}
        </section>
      )}

      {dialog && (
        <GroupDialog
          mode={dialog.mode}
          group={dialog.mode === "edit" ? dialog.group : undefined}
          agents={settings.agents}
          onSubmit={async (name, topic, members) => {
            if (dialog.mode === "create") {
              await group.create(name, topic, members);
            } else {
              await group.configure(dialog.group.id, name, topic, members);
            }
            setDialog(null);
          }}
          onDelete={
            dialog.mode === "edit"
              ? async () => {
                  await group.deleteGroup(dialog.group.id);
                  setDialog(null);
                }
              : undefined
          }
          onClose={() => setDialog(null)}
        />
      )}
    </div>
  );
}
