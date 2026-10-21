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
import { ChevronRight, CheckIcon } from "./Icons";
import { formatHm } from "../utils/format";
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

/** One row inside a merged activity card: a thought or one tool call. */
type ActivityEntry =
  | { kind: "thought"; key: string; text: string; streaming?: boolean }
  | { kind: "tool"; key: string; name: string; arguments: unknown; result?: unknown; isRunning?: boolean };

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

/** Flatten the thread into blocks. A whole batch — the thinking and every tool
 *  call between two answers — becomes ONE activity card instead of a box per
 *  row. `mergeActivities=false` keeps each item separate (selection mode, where
 *  every item needs its own clickable row). */
function buildBlocks(items: ChatItem[], liveToolCalls: ToolCall[], mergeActivities: boolean): ThreadBlock[] {
  const blocks: ThreadBlock[] = [];

  const pushActivity = (entries: ActivityEntry[], ts?: number) => {
    const last = blocks[blocks.length - 1];
    if (mergeActivities && last?.kind === "activity") {
      last.entries.push(...entries);
      last.endTs = ts ?? last.endTs;
      return;
    }
    blocks.push({ kind: "activity", key: `activity-${entries[0].key}`, entries, startTs: ts, endTs: ts });
  };

  items.forEach((item, index) => {
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
      );
      return;
    }
    if (isThoughtOnly(item)) {
      pushActivity([{ kind: "thought", key: item.id, text: item.reasoning! }], item.ts);
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
    );
  }

  return blocks;
}

/** One card holding all thoughts and tool calls of the same batch. */
function ActivityBlock({ entries }: { entries: ActivityEntry[] }) {
  return (
    <div className="my-0.5 overflow-hidden rounded-field border border-line bg-surface-soft text-body">
      {entries.map((entry, i) => (
        <div key={entry.key} className={i > 0 ? "border-t border-line/70" : ""}>
          {entry.kind === "thought" ? (
            <ReasoningRow text={entry.text} streaming={entry.streaming} />
          ) : (
            <ToolCallBlock
              grouped
              name={entry.name}
              arguments={entry.arguments}
              result={entry.result}
              isRunning={entry.isRunning}
            />
          )}
        </div>
      ))}
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

  return (
    <div className={`flex flex-col overflow-y-auto ${names ? "gap-4" : "gap-2"} ${className}`}>
      {isEmpty && emptyHint && <div className="mt-10 text-center text-chat text-ink-faint">{emptyHint}</div>}

      {blocksWithTime.map(({ block, showTime }) => {
        if (block.kind === "activity") {
          return (
            <div key={block.key} className="flex flex-col gap-2">
              {!names && showTime && block.startTs !== undefined && (
                <div className="self-center px-2 py-0.5 text-meta text-ink-faint">{formatHm(block.startTs)}</div>
              )}
              <div className="max-w-[85%]">
                <ActivityBlock entries={block.entries} />
              </div>
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

      {showStreaming && (
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
