import { useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import type { ChatItem, ToolCall } from "../hooks/useChat";
import { MessageBubble } from "./ui/MessageBubble";
import { ReasoningBlock } from "./ui/ReasoningBlock";
import { CodeBlock } from "./ui/CodeBlock";
import { Markdown } from "./ui/Markdown";
import { ToolCallBlock } from "./panel/ToolCallBlock";
import { ChevronRight, CheckIcon } from "./Icons";
import { formatHm, formatJson } from "../utils/format";
import { useI18n } from "../i18n";

interface Props {
  items: ChatItem[];
  currentToolCalls: ToolCall[];
  streaming: string; // in-progress assistant text (empty when idle)
  streamingReasoning?: string; // in-progress chain-of-thought (empty when idle)
  loading: boolean;
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
}

/** Sender labels for the meta row; absent in the compact (pet window) mode. */
interface Names {
  user: string;
  assistant: string;
}

const FIVE_MIN = 5 * 60 * 1000;

/** The "后台任务完成：XXX" system line. Click to expand the task's full result. */
function NotificationItem({ content, detail }: { content: string; detail?: string }) {
  const [expanded, setExpanded] = useState(false);
  const hasDetail = !!detail;
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
        <span>{content}</span>
      </button>
      {expanded && hasDetail && (
        <CodeBlock className="mt-1 w-full">{formatJson(detail!)}</CodeBlock>
      )}
    </div>
  );
}

function renderItem(item: ChatItem, names?: Names) {
  switch (item.type) {
    case "user":
      return (
        <MessageBubble role="user" images={item.images} name={names?.user} ts={item.ts}>
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
        <MessageBubble role="assistant" images={item.images} name={names?.assistant} ts={item.ts}>
          {hasReasoning && <ReasoningBlock text={item.reasoning!} />}
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
        <MessageBubble role="assistant" error name={names?.assistant} ts={item.ts}>
          {item.content}
        </MessageBubble>
      );
    case "notification":
      // A subtle system line (not a chat bubble) marking an auto-resumed turn;
      // expandable to view the task's full result.
      return <NotificationItem content={item.content} detail={item.detail} />;
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
  className = "",
  emptyHint,
  assistantName,
  selectionMode = false,
  selectedKeys,
  onToggleSelect,
}: Props) {
  const { t } = useI18n();
  const endRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    endRef.current?.scrollIntoView({ block: "end" });
  }, [items, streaming, streamingReasoning, currentToolCalls, loading]);

  const names: Names | undefined = assistantName
    ? { user: t("chat.you"), assistant: assistantName }
    : undefined;

  const showStreaming = streaming.trim().length > 0 || streamingReasoning.trim().length > 0;
  const isEmpty = items.length === 0 && !showStreaming && !loading;
  if (isEmpty && !emptyHint) return null;

  return (
    <div className={`flex flex-col overflow-y-auto ${names ? "gap-4" : "gap-2"} ${className}`}>
      {isEmpty && emptyHint && <div className="mt-10 text-center text-chat text-ink-faint">{emptyHint}</div>}

      {items.map((item, i) => {
        const prev = items[i - 1];
        // With a per-message meta row the time is already on every message, so
        // the periodic separator would just repeat it.
        const showTime =
          !names && item.ts !== undefined && (i === 0 || prev?.ts === undefined || item.ts - prev.ts > FIVE_MIN);
        const selected = item.id ? (selectedKeys?.has(item.id) ?? false) : false;
        return (
          <div key={item.id ?? i} className="flex flex-col gap-2">
            {showTime && (
              <div className="self-center px-2 py-0.5 text-meta text-ink-faint">{formatHm(item.ts!)}</div>
            )}
            {selectionMode ? (
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
                <div className="min-w-0 flex-1 pointer-events-none">{renderItem(item, names)}</div>
              </button>
            ) : (
              renderItem(item, names)
            )}
          </div>
        );
      })}

      {currentToolCalls.length > 0 && (
        <div className="max-w-[85%]">
          {currentToolCalls.map((tc, j) => (
            <ToolCallBlock key={j} name={tc.name} arguments={tc.arguments} result={tc.result} isRunning={tc.isRunning} />
          ))}
        </div>
      )}

      {showStreaming && (
        <MessageBubble role="assistant" name={names?.assistant}>
          {streamingReasoning.trim() && <ReasoningBlock text={streamingReasoning} streaming />}
          {streaming.trim() && <Markdown text={streaming} caret />}
        </MessageBubble>
      )}

      {loading && !showStreaming && currentToolCalls.length === 0 && (
        <div className="flex gap-1 self-start rounded-bubble border border-line bg-surface px-3.5 py-3 shadow-card">
          <span className="h-1.5 w-1.5 animate-bounce rounded-full bg-ink-faint [animation-delay:-0.2s]" />
          <span className="h-1.5 w-1.5 animate-bounce rounded-full bg-ink-faint [animation-delay:-0.1s]" />
          <span className="h-1.5 w-1.5 animate-bounce rounded-full bg-ink-faint" />
        </div>
      )}

      <div ref={endRef} />
    </div>
  );
}
