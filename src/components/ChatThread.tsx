import { useEffect, useMemo, useRef, useState } from "react";
import type { ReactNode } from "react";
import type { ChatItem, ToolCall } from "../hooks/useChat";
import { MessageBubble } from "./ui/MessageBubble";
import { MessageEditor } from "./ui/MessageEditor";
import { ReasoningRow } from "./ui/ReasoningRow";
import { JsonView } from "./ui/JsonView";
import { Markdown } from "./ui/Markdown";
import { ToolCallBlock } from "./ui/ToolCallBlock";
import { TurnStatus } from "./ui/TurnStatus";
import { ChevronRight, CheckIcon, ExpandChevron, LightbulbIcon, SpinnerIcon, WrenchIcon } from "./Icons";
import { describeToolCall } from "../utils/toolDisplay";
import { formatElapsed, formatHm } from "../utils/format";
import { useI18n } from "../i18n";

interface Props {
  items: ChatItem[];
  currentToolCalls: ToolCall[];
  streaming: string; // in-progress assistant text (empty when idle)
  streamingReasoning?: string; // in-progress chain-of-thought (empty when idle)
  loading: boolean;
  /** Epoch ms the running turn started, and what it has burned so far — the
   *  running indicator's figures. Omitted by views without a turn of their own
   *  (the group tabs), which then show only what the turn is doing. */
  turnStartedAt?: number;
  turnTokens?: number;
  /** Extra classes for the scroll container (controls bg/padding/position). */
  className?: string;
  /** Shown when there are no messages. If omitted, the whole thread renders nothing when empty. */
  emptyHint?: ReactNode;
  /** Display name of the pet. When set, every message gets a sender + time meta
   *  row (and the assistant an avatar) instead of the compact bubbles-only look
   *  the pet window uses; the periodic time separators are then redundant. */
  assistantName?: string;
  /** When true, each row shows a checkbox and clicking it toggles selection. */
  selectionMode?: boolean;
  /** Stable item ids currently selected (never array indices — they shift). */
  selectedKeys?: Set<string>;
  /** Toggle selection for the item with id `id`. */
  onToggleSelect?: (id: string) => void;
  /** Rewrite the user message `id` and resend it, dropping everything after it.
   *  Omitted by read-only threads (the group tabs). */
  onEditMessage?: (id: string, text: string) => void;
}

/** Sender labels for the meta row; absent in the compact (pet window) mode. */
interface Names {
  user: string;
  assistant: string;
}

const FIVE_MIN = 5 * 60 * 1000;

/** One row inside a merged activity card: a thought, one tool call, or the
 *  batch's text (the last text row becomes the answer bubble). */
type ActivityEntry =
  | { kind: "thought"; key: string; text: string; streaming?: boolean }
  | { kind: "tool"; key: string; name: string; arguments: unknown; result?: unknown; isRunning?: boolean }
  | { kind: "text"; key: string; content: string; images?: string[]; ts?: number };

/** Consecutive activity items collapse into one card; every other item keeps
 *  its own bubble. `startTs`/`endTs` drive the periodic time separator. */
type ThreadBlock =
  | { kind: "activity"; key: string; entries: ActivityEntry[]; startTs?: number; endTs?: number }
  | { kind: "item"; key: string; item: ChatItem; index: number; startTs: number; endTs: number };

/** A thought-only assistant item (the model reasoned, then called a tool
 *  without any preamble text) belongs in the activity card, not in a bubble. */
function isThoughtOnly(item: ChatItem): boolean {
  return item.type === "assistant" && !!item.reasoning?.trim() && !item.content.trim() && !item.images?.length;
}

/** Flatten the thread into blocks. A whole batch — the thinking, every tool
 *  call and the final answer between two user messages — becomes ONE chat
 *  block: the sender's name once on top, the activity rows, then the answer
 *  as its own bubble. `mergeActivities=false` keeps each item separate
 *  (selection mode, where every item needs its own clickable row). */
function buildBlocks(items: ChatItem[], liveToolCalls: ToolCall[], mergeActivities: boolean): ThreadBlock[] {
  const blocks: ThreadBlock[] = [];

  // The activity of a turn starts when its user message (or the system line
  // that auto-resumed it) does — not when the first thought/tool is committed,
  // which would drop the model's initial thinking from the elapsed time.
  let turnStartTs: number | undefined;

  const lastActivity = () => {
    const last = blocks[blocks.length - 1];
    return last?.kind === "activity" ? last : undefined;
  };

  const pushActivity = (entries: ActivityEntry[], ts?: number, startTs?: number) => {
    const last = lastActivity();
    if (mergeActivities && last) {
      last.entries.push(...entries);
      last.endTs = ts ?? last.endTs;
      return;
    }
    blocks.push({ kind: "activity", key: `activity-${entries[0].key}`, entries, startTs: startTs ?? ts, endTs: ts });
  };

  items.forEach((item, index) => {
    if (item.type === "user" || item.type === "notification") turnStartTs = item.ts;
    if (item.type === "tool" && item.toolCalls?.length) {
      pushActivity(
        item.toolCalls.map((tc, j) => ({
          kind: "tool",
          key: `${item.id}-${j}`,
          name: tc.name,
          arguments: tc.arguments,
          result: tc.result,
          isRunning: tc.isRunning,
        })),
        item.ts,
        turnStartTs,
      );
      return;
    }
    if (isThoughtOnly(item)) {
      pushActivity([{ kind: "thought", key: item.id, text: item.reasoning! }], item.ts, turnStartTs);
      return;
    }

    // The answer to a batch belongs to the same chat block: attach it to the
    // activity card it follows instead of starting a second bubble that would
    // repeat the sender's name. Its last-round thinking joins the card too.
    if (mergeActivities && item.type === "assistant" && lastActivity() && (item.content.trim() || item.images?.length)) {
      if (item.reasoning?.trim()) {
        pushActivity([{ kind: "thought", key: `reasoning-${item.id}`, text: item.reasoning }], item.ts, turnStartTs);
      }
      const block = lastActivity()!;
      block.entries.push({
        kind: "text",
        key: `text-${item.id ?? index}`,
        content: item.content,
        images: item.images,
        ts: item.ts,
      });
      block.endTs = item.ts;
      return;
    }

    blocks.push({
      kind: "item",
      key: item.id ?? `item-${index}`,
      item,
      index,
      startTs: item.ts,
      endTs: item.ts,
    });
  });

  // Tool calls still streaming. They are the newest activity, so they join the
  // card above when it is an activity card — the batch keeps one container from
  // the first thought to the last result.
  if (mergeActivities && liveToolCalls.length > 0) {
    pushActivity(
      liveToolCalls.map((tc, j) => ({
        kind: "tool",
        key: `live-${j}`,
        name: tc.name,
        arguments: tc.arguments,
        result: tc.result,
        isRunning: tc.isRunning,
      })),
      undefined,
      turnStartTs,
    );
  }

  return blocks;
}

/** The one-line preview used by the collapsed activity card: the latest
 *  thought is markdown source, so drop the markers that would show as literal
 *  text in a plain span. */
function previewLine(text: string): string {
  const lines = text
    .split("\n")
    .map((line) =>
      line
        .trim()
        .replace(/^#{1,6}\s+/, "")
        .replace(/^\s*[-*+]\s+/, "")
        .replace(/\*\*|__|~~|`/g, "")
        .trim(),
    )
    .filter(Boolean);
  return lines[lines.length - 1] ?? "";
}

/** Re-renders once a second while `active`, so a running block's elapsed time
 *  ticks instead of freezing at the moment it mounted. */
function useTicker(active: boolean): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!active) return;
    setNow(Date.now());
    const id = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(id);
  }, [active]);
  return now;
}

/** One chat block: the sender's name once on top, then a card with every
 *  thought/tool call of the batch, then the answer as its own bubble and its
 *  timestamp. The activity card is folded to a summary by default; while the
 *  turn is still working, its latest thought/tool call shimmers in the header.
 *  The last text row is pulled out of the card to serve as the answer — the
 *  card keeps only the activity that led to it. */
function ActivityBlock({
  entries,
  name,
  running = false,
  startedAt,
  endedAt,
  streamingText = "",
  streamingReasoning = "",
}: {
  entries: ActivityEntry[];
  name?: string;
  /** The turn is still running on this block (latest activity keeps updating). */
  running?: boolean;
  /** When the turn started and when its last row landed — the elapsed label. */
  startedAt?: number;
  endedAt?: number;
  /** Answer text still streaming in; rendered as the open output bubble. */
  streamingText?: string;
  streamingReasoning?: string;
}) {
  const { t } = useI18n();
  const [expanded, setExpanded] = useState(false);
  const now = useTicker(running);

  const rows: ActivityEntry[] = [...entries];
  const liveThought = streamingReasoning.trim();
  if (liveThought) {
    rows.push({ kind: "thought", key: "live-thought", text: streamingReasoning, streaming: true });
  }
  if (streamingText.trim()) {
    rows.push({ kind: "text", key: "live-output", content: streamingText });
  }

  const last = rows[rows.length - 1];
  const output = last?.kind === "text" ? last : undefined;
  const body = output ? rows.slice(0, -1) : rows;

  // Counts come from the committed rows plus the thought still streaming — so
  // the number ticks up as a new thinking round starts, not only when a tool
  // finally forces that thought to be committed.
  const thoughts = entries.filter((entry) => entry.kind === "thought").length + (liveThought ? 1 : 0);
  const tools = entries.filter((entry) => entry.kind === "tool").length;
  const ended = running ? now : endedAt;
  const elapsed = startedAt !== undefined && ended !== undefined ? formatElapsed(ended - startedAt, t) : undefined;

  // The newest thing the model is doing. The streaming thought wins (it has not
  // been committed yet); otherwise fall back to the last committed row.
  let latest: { icon: ReactNode; text: string } | undefined;
  if (liveThought) {
    latest = { icon: <LightbulbIcon className="h-3.5 w-3.5 shrink-0 text-ink-faint" />, text: previewLine(streamingReasoning) };
  } else {
    const latestEntry = entries[entries.length - 1];
    if (latestEntry?.kind === "thought") {
      latest = { icon: <LightbulbIcon className="h-3.5 w-3.5 shrink-0 text-ink-faint" />, text: previewLine(latestEntry.text) };
    } else if (latestEntry?.kind === "tool") {
      const { Icon, label, summary: detail } = describeToolCall(latestEntry.name, latestEntry.arguments);
      latest = { icon: <Icon className="h-3.5 w-3.5 shrink-0 text-ink-faint" />, text: [label, detail].filter(Boolean).join(" ") };
    } else if (latestEntry?.kind === "text") {
      latest = { icon: null, text: previewLine(latestEntry.content) };
    }
  }
  const active = running && !streamingText.trim();

  const entryRow = (entry: ActivityEntry) =>
    entry.kind === "thought" ? (
      <ReasoningRow text={entry.text} streaming={entry.streaming} />
    ) : entry.kind === "text" ? (
      <div className="px-3 py-2">
        {entry.images?.map((url, k) => (
          <img key={k} src={url} alt="" className="mb-1.5 max-w-full rounded-lg object-contain" />
        ))}
        {entry.content.trim() && <Markdown text={entry.content} />}
      </div>
    ) : (
      <ToolCallBlock
        grouped
        name={entry.name}
        arguments={entry.arguments}
        result={entry.result}
        isRunning={entry.isRunning}
      />
    );

  // A single thought/tool call is already one glanceable row — folding it would
  // hide the content behind a summary that says the same thing.
  const single = body.length === 1 ? body[0] : undefined;

  return (
    <div className="min-w-0">
      {name && <div className="mb-1 px-0.5 text-meta font-medium text-ink-soft">{name}</div>}
      {single ? (
        <div className="my-0.5 max-w-[85%] overflow-hidden rounded-field border border-line bg-surface-soft text-body">
          {entryRow(single)}
        </div>
      ) : (
        body.length > 0 && (
          <div className="my-0.5 max-w-[85%] overflow-hidden rounded-field border border-line bg-surface-soft text-body">
            <button
              type="button"
              aria-expanded={expanded}
              onClick={() => setExpanded((value) => !value)}
              className="flex w-full items-center gap-2 px-3 py-1.5 text-left transition-colors hover:bg-hover"
            >
              {/* Elapsed time leads and carries the emphasis; the activity glyphs
                  and counts sit beside it in a quieter tone. */}
              {elapsed && (
                <span className="shrink-0 text-body font-semibold text-accent tabular-nums">
                  {t("chat.activity.duration", { duration: elapsed })}
                </span>
              )}
              <span className="flex shrink-0 items-center gap-2.5 text-ink-faint">
                {thoughts > 0 && (
                  <span
                    className="flex items-center gap-1"
                    title={t("chat.activity.thoughts", { count: thoughts })}
                    aria-label={t("chat.activity.thoughts", { count: thoughts })}
                  >
                    <LightbulbIcon className="h-3.5 w-3.5" />
                    <span className="text-note font-medium tabular-nums">{thoughts}</span>
                  </span>
                )}
                {tools > 0 && (
                  <span
                    className="flex items-center gap-1"
                    title={t("chat.activity.tools", { count: tools })}
                    aria-label={t("chat.activity.tools", { count: tools })}
                  >
                    <WrenchIcon className="h-3.5 w-3.5" />
                    <span className="text-note font-medium tabular-nums">{tools}</span>
                  </span>
                )}
              </span>
              {active && (
                <span className="flex min-w-0 flex-1 items-center gap-1.5 text-note">
                  {latest?.icon}
                  <span className="activity-shimmer min-w-0 truncate">
                    {latest?.text || t("chat.activity.working")}
                  </span>
                </span>
              )}
              {running && <SpinnerIcon className="ml-auto h-3.5 w-3.5 shrink-0 animate-spin text-ink-faint" />}
              <ExpandChevron expanded={expanded} className="h-3.5 w-3.5 shrink-0 text-ink-faint" />
            </button>
            {expanded &&
              body.map((entry) => (
                <div key={entry.key} className="border-t border-line/70">
                  {entryRow(entry)}
                </div>
              ))}
          </div>
        )
      )}
      {output && (
        <div className="mt-2">
          <MessageBubble
            role="assistant"
            meta={!!name}
            images={output.images}
            ts={output.ts}
            copyText={output.content}
          >
            {output.content.trim() && <Markdown text={output.content} />}
          </MessageBubble>
        </div>
      )}
    </div>
  );
}

/** The "后台任务完成：XXX" system line. Click to expand the task's full result. */
function NotificationItem({ content, label, detail }: { content: string; label?: string; detail?: string }) {
  const { t } = useI18n();
  const [expanded, setExpanded] = useState(false);
  const hasDetail = !!detail;
  // The backend stores the task label; the line itself is rendered in the
  // window's language (`content` is the plain fallback the CLI shows).
  const line = label ? t("chat.bgTaskDone", { label }) : content;
  return (
    <div className="flex w-full max-w-[90%] flex-col items-center self-center">
      <button
        type="button"
        disabled={!hasDetail}
        onClick={() => setExpanded((e) => !e)}
        className={`flex items-center gap-1 rounded-full bg-surface-soft px-3 py-1 text-note text-ink-soft ${
          hasDetail ? "hover:bg-hover" : "cursor-default"
        }`}
      >
        {hasDetail && (
          <ChevronRight className={`h-3 w-3 shrink-0 transition-transform ${expanded ? "rotate-90" : ""}`} />
        )}
        <span>{line}</span>
      </button>
      {expanded && hasDetail && (
        <div className="mt-1 max-h-[280px] w-full overflow-y-auto rounded-field border border-line bg-surface-soft px-2.5 py-2">
          <JsonView value={detail} />
        </div>
      )}
    </div>
  );
}

/** `actionable` is false in selection mode, where the row itself owns the click
 *  (and the buttons would also nest a button inside the row's button — `onEdit`
 *  is likewise left off there). */
function renderItem(item: ChatItem, names?: Names, actionable = true, onEdit?: () => void) {
  const copyText = actionable ? item.content : undefined;
  switch (item.type) {
    case "user":
      return (
        <MessageBubble
          role="user"
          images={item.images}
          name={names?.user}
          ts={item.ts}
          copyText={copyText}
          onEdit={onEdit}
        >
          {item.content}
        </MessageBubble>
      );
    case "assistant": {
      // Tool-produced images (e.g. screenshots) arrive as assistant items with
      // empty text — still render the bubble so the image shows. A reasoning-only
      // item (model thought, then called a tool with no preamble) still renders
      // so its thinking is viewable.
      const hasReasoning = !!item.reasoning?.trim();
      if (!item.content.trim() && !item.images?.length && !hasReasoning) return null;
      return (
        <MessageBubble
          role="assistant"
          images={item.images}
          name={names?.assistant}
          ts={item.ts}
          copyText={copyText}
        >
          {hasReasoning && <ReasoningRow embedded text={item.reasoning!} />}
          {item.content.trim() && <Markdown text={item.content} />}
        </MessageBubble>
      );
    }
    case "tool":
      return (
        <div className="max-w-[85%]">
          {item.toolCalls?.map((tc, j) => (
            <ToolCallBlock key={j} name={tc.name} arguments={tc.arguments} result={tc.result} />
          ))}
        </div>
      );
    case "error":
      return (
        <MessageBubble role="assistant" error name={names?.assistant} ts={item.ts} copyText={copyText}>
          {item.content}
        </MessageBubble>
      );
    case "notification":
      // A subtle system line (not a chat bubble) marking an auto-resumed turn;
      // expandable to view the task's full result.
      return <NotificationItem content={item.content} label={item.label} detail={item.detail} />;
    default:
      return null;
  }
}

/** Shared chat message list: renders items (incl. tool calls), live tool calls,
 *  streaming response and timestamps. Identical logic for the pet and panel
 *  windows — only `className` and the meta rows differ. */
export function ChatThread({
  items,
  currentToolCalls,
  streaming,
  streamingReasoning = "",
  loading,
  turnStartedAt,
  turnTokens,
  className = "",
  emptyHint,
  assistantName,
  selectionMode = false,
  selectedKeys,
  onToggleSelect,
  onEditMessage,
}: Props) {
  const { t } = useI18n();
  const endRef = useRef<HTMLDivElement>(null);
  // Id of the message open in the composer (one at a time).
  const [editingId, setEditingId] = useState<string | null>(null);

  useEffect(() => {
    endRef.current?.scrollIntoView({ block: "end" });
  }, [items, streaming, streamingReasoning, currentToolCalls, loading]);

  const names: Names | undefined = assistantName
    ? { user: t("chat.you"), assistant: assistantName }
    : undefined;

  const showStreaming = streaming.trim().length > 0 || streamingReasoning.trim().length > 0;
  const isEmpty = items.length === 0 && !showStreaming && !loading;

  // Merge each batch's thinking + tool calls into one card. Selection mode
  // needs one clickable row per item, so it opts out.
  const blocks = useMemo(
    () =>
      selectionMode
        ? items.map((item, index) => ({
            kind: "item" as const,
            key: item.id ?? `item-${index}`,
            item,
            index,
            startTs: item.ts,
            endTs: item.ts,
          }))
        : buildBlocks(items, currentToolCalls, true),
    [items, currentToolCalls, selectionMode],
  );

  // A time chip appears before the block that follows a >5min gap.
  const blocksWithTime = useMemo(() => {
    let prevTs: number | undefined;
    return blocks.map((block) => {
      const showTime = block.startTs !== undefined && (prevTs === undefined || block.startTs - prevTs > FIVE_MIN);
      if (block.endTs !== undefined) prevTs = block.endTs;
      return { block, showTime };
    });
  }, [blocks]);

  if (isEmpty && !emptyHint) return null;

  // The live answer joins the open activity card, so the agent's name stays at
  // the top of one block instead of repeating above a second bubble.
  const streamingInCard = !selectionMode && blocks[blocks.length - 1]?.kind === "activity";

  return (
    <div className={`flex flex-col overflow-y-auto ${names ? "gap-4" : "gap-2"} ${className}`}>
      {isEmpty && emptyHint && <div className="mt-10 text-center text-chat text-ink-faint">{emptyHint}</div>}

      {blocksWithTime.map(({ block, showTime }, i) => {
        if (block.kind === "activity") {
          const live = streamingInCard && i === blocksWithTime.length - 1;
          return (
            <div key={block.key} className="flex flex-col gap-2">
              {!names && showTime && block.startTs !== undefined && (
                <div className="self-center px-2 py-0.5 text-meta text-ink-faint">{formatHm(block.startTs)}</div>
              )}
              <ActivityBlock
                entries={block.entries}
                name={names?.assistant}
                running={live && loading}
                startedAt={block.startTs}
                endedAt={block.endTs}
                streamingText={live ? streaming : ""}
                streamingReasoning={live ? streamingReasoning : ""}
              />
            </div>
          );
        }

        const item = block.item;
        // With a per-message meta row the time is already on every message, so
        // the periodic separator would just repeat it.
        const selected = item.id ? (selectedKeys?.has(item.id) ?? false) : false;
        // Editing means resending, so it is offered only on a user message, and
        // only while the session is idle — a running turn would refuse both the
        // prune and the send.
        const editable = !!onEditMessage && item.type === "user" && !!item.id && !loading;
        return (
          <div key={block.key} className="flex flex-col gap-2">
            {!names && showTime && (
              <div className="self-center px-2 py-0.5 text-meta text-ink-faint">{formatHm(block.startTs)}</div>
            )}
            {editingId === item.id ? (
              <MessageEditor
                text={item.content}
                dropCount={items.slice(block.index + 1).filter((it) => it.type !== "tool").length}
                onCancel={() => setEditingId(null)}
                onSubmit={(text) => {
                  setEditingId(null);
                  onEditMessage?.(item.id, text);
                }}
              />
            ) : selectionMode ? (
              <button
                type="button"
                onClick={() => item.id && onToggleSelect?.(item.id)}
                className={`flex w-full items-start gap-2 rounded-field p-1.5 text-left transition-colors ${
                  selected ? "bg-accent-soft ring-1 ring-accent" : "hover:bg-hover"
                }`}
              >
                <span
                  className={`mt-1 flex h-4 w-4 shrink-0 items-center justify-center rounded border ${
                    selected ? "border-accent bg-accent text-white" : "border-line bg-surface"
                  }`}
                >
                  {selected && <CheckIcon className="h-3 w-3" />}
                </span>
                {/* Disable inner pointer events so the row click owns the toggle. */}
                <div className="min-w-0 flex-1 pointer-events-none">{renderItem(item, names, false)}</div>
              </button>
            ) : (
              renderItem(item, names, true, editable ? () => setEditingId(item.id) : undefined)
            )}
          </div>
        );
      })}

      {/* In normal mode live calls join the batch card above; only selection mode
          (which opts out of merging) still renders them on their own. */}
      {selectionMode && currentToolCalls.length > 0 && (
        <div className="max-w-[85%]">
          {currentToolCalls.map((tc, j) => (
            <ToolCallBlock key={j} name={tc.name} arguments={tc.arguments} result={tc.result} isRunning={tc.isRunning} />
          ))}
        </div>
      )}

      {showStreaming && !streamingInCard && (
        <MessageBubble role="assistant" name={names?.assistant}>
          {streamingReasoning.trim() && <ReasoningRow embedded text={streamingReasoning} streaming />}
          {streaming.trim() && <Markdown text={streaming} />}
        </MessageBubble>
      )}

      {/* "The turn is still running" — the same state the input's stop button
          shows. It stays up through every phase of a turn (waiting for the
          first token, between rounds, while a tool runs), so it must follow
          `loading` alone — gating it on "nothing streamed yet" made it flash
          once at the start of a turn and never come back. */}
      {loading && (
        <TurnStatus
          startedAt={turnStartedAt}
          tokens={turnTokens}
          toolCalls={currentToolCalls}
          streaming={streaming}
          reasoning={streamingReasoning}
        />
      )}

      <div ref={endRef} />
    </div>
  );
}
